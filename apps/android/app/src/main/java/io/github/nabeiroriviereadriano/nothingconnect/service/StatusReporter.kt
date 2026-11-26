package io.github.nabeiroriviereadriano.nothingconnect.service

import android.app.NotificationManager
import android.bluetooth.BluetoothManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioManager
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.BatteryManager
import android.os.Environment
import android.os.Handler
import android.os.Looper
import android.os.StatFs
import android.telephony.TelephonyManager
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import org.json.JSONObject

@android.annotation.SuppressLint("MissingPermission")
/** Envoie l'état du téléphone : au branchement, sur changement de batterie, puis toutes les 60 s. */
class StatusReporter(private val ctx: Context) {
    private val handler = Handler(Looper.getMainLooper())
    private var lastSent = 0L
    private val tick = object : Runnable {
        override fun run() {
            sendNow(null)
            handler.postDelayed(this, 60_000)
        }
    }
    private val battery = object : BroadcastReceiver() {
        override fun onReceive(c: Context, i: Intent) {
            // Limite à un envoi toutes les 20 s sur simple variation de batterie.
            if (System.currentTimeMillis() - lastSent > 20_000) sendNow(null)
        }
    }

    fun start() {
        ctx.registerReceiver(battery, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        handler.postDelayed(tick, 60_000)
    }

    fun stop() {
        handler.removeCallbacks(tick)
        runCatching { ctx.unregisterReceiver(battery) }
    }

    /** `peer` nul : à tous les PC connectés. */
    fun sendNow(peer: String?) {
        if (Core.connected.value.isEmpty()) return
        lastSent = System.currentTimeMillis()
        val msg = JSONObject().put("t", "status")
        val s = collect()
        s.keys().forEach { k -> msg.put(k, s.get(k)) }
        if (peer != null) Core.send(peer, msg) else Core.broadcast(msg)
    }

    private fun collect(): JSONObject {
        val o = JSONObject()
        ctx.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))?.let { b ->
            val level = b.getIntExtra(BatteryManager.EXTRA_LEVEL, -1)
            val scale = b.getIntExtra(BatteryManager.EXTRA_SCALE, 100)
            if (level >= 0 && scale > 0) o.put("battery_percent", level * 100 / scale)
            val st = b.getIntExtra(BatteryManager.EXTRA_STATUS, -1)
            o.put("charging", st == BatteryManager.BATTERY_STATUS_CHARGING || st == BatteryManager.BATTERY_STATUS_FULL)
            val t = b.getIntExtra(BatteryManager.EXTRA_TEMPERATURE, Int.MIN_VALUE)
            if (t != Int.MIN_VALUE) o.put("battery_temp_c", t / 10.0)
        }
        runCatching {
            val fs = StatFs(Environment.getDataDirectory().path)
            o.put("storage_free_bytes", fs.availableBytes).put("storage_total_bytes", fs.totalBytes)
        }
        runCatching {
            val cm = ctx.getSystemService(ConnectivityManager::class.java)
            val caps = cm.getNetworkCapabilities(cm.activeNetwork)
            o.put("wifi_connected", caps?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true)
        }
        runCatching { o.put("bluetooth_enabled", ctx.getSystemService(BluetoothManager::class.java)?.adapter?.isEnabled == true) }
        runCatching {
            val name = ctx.getSystemService(TelephonyManager::class.java)?.networkOperatorName
            if (!name.isNullOrBlank()) o.put("mobile_network", name)
        }
        runCatching {
            val am = ctx.getSystemService(AudioManager::class.java)
            val max = am.getStreamMaxVolume(AudioManager.STREAM_MUSIC)
            if (max > 0) o.put("media_volume_percent", am.getStreamVolume(AudioManager.STREAM_MUSIC) * 100 / max)
        }
        runCatching {
            val f = ctx.getSystemService(NotificationManager::class.java).currentInterruptionFilter
            o.put("dnd_enabled", f != NotificationManager.INTERRUPTION_FILTER_ALL && f != NotificationManager.INTERRUPTION_FILTER_UNKNOWN)
        }
        runCatching { o.put("display_max_hz", io.github.nabeiroriviereadriano.nothingconnect.stream.DisplayActivity.maxRefreshRate(ctx)) }
        return o
    }
}
