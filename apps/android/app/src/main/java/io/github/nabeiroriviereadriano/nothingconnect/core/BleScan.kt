package io.github.nabeiroriviereadriano.nothingconnect.core

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothManager
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import kotlinx.coroutines.delay

/** PC annoncé en Bluetooth basse consommation (repli quand la découverte Wi-Fi échoue). */
data class BlePc(val fingerprint: String, val addr: String)

/**
 * Recherche des PC NOTHING CONNECT en Bluetooth LE. Le PC diffuse son empreinte et son
 * adresse Wi-Fi ; la connexion se fait ensuite sur le réseau local, chiffrée comme toujours.
 * Android 12+ seulement (autorisation « Appareils à proximité », sans localisation).
 */
object BleScan {
    private const val COMPANY = 0xFFFF

    fun available(ctx: Context): Boolean = Build.VERSION.SDK_INT >= 31 &&
        ctx.getSystemService(BluetoothManager::class.java)?.adapter?.isEnabled == true

    fun permitted(ctx: Context): Boolean = Build.VERSION.SDK_INT >= 31 &&
        ctx.checkSelfPermission(Manifest.permission.BLUETOOTH_SCAN) == PackageManager.PERMISSION_GRANTED

    /** Décodage : "NC" + 8 octets d'empreinte + IPv4 + port (big-endian). */
    fun parse(data: ByteArray): BlePc? {
        if (data.size < 16 || data[0] != 'N'.code.toByte() || data[1] != 'C'.code.toByte()) return null
        val hex = (2 until 10).joinToString("") { "%02X".format(data[it]) }
        val fp = hex.chunked(4).joinToString("-")
        val ip = (10 until 14).joinToString(".") { (data[it].toInt() and 0xff).toString() }
        val port = ((data[14].toInt() and 0xff) shl 8) or (data[15].toInt() and 0xff)
        return BlePc(fp, "$ip:$port")
    }

    @SuppressLint("MissingPermission")
    suspend fun scan(ctx: Context, millis: Long): List<BlePc> {
        if (!available(ctx) || !permitted(ctx)) return emptyList()
        val scanner = ctx.getSystemService(BluetoothManager::class.java).adapter.bluetoothLeScanner ?: return emptyList()
        val found = LinkedHashMap<String, BlePc>()
        val cb = object : ScanCallback() {
            override fun onScanResult(callbackType: Int, result: ScanResult) {
                val d = result.scanRecord?.getManufacturerSpecificData(COMPANY) ?: return
                parse(d)?.let { synchronized(found) { found[it.fingerprint] = it } }
            }
        }
        val filter = ScanFilter.Builder().setManufacturerData(COMPANY, byteArrayOf('N'.code.toByte(), 'C'.code.toByte()), byteArrayOf(-1, -1)).build()
        val settings = ScanSettings.Builder().setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY).build()
        runCatching { scanner.startScan(listOf(filter), settings, cb) }.onFailure { return emptyList() }
        try {
            delay(millis)
        } finally {
            runCatching { scanner.stopScan(cb) }
        }
        return synchronized(found) { found.values.toList() }
    }
}
