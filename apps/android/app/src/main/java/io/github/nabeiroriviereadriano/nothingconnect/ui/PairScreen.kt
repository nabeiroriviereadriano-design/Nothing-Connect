package io.github.nabeiroriviereadriano.nothingconnect.ui

import android.content.Context
import android.net.wifi.WifiManager
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import io.github.nabeiroriviereadriano.nothingconnect.core.PairingUi
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import androidx.activity.result.contract.ActivityResultContracts
import io.github.nabeiroriviereadriano.nothingconnect.core.BleScan
import uniffi.nc_ffi.DiscoveredPc
import uniffi.nc_ffi.discoverPcs

/** Association : PC trouvés automatiquement sur le Wi-Fi + code, ou QR code, ou adresse manuelle. */
@Composable
fun PairScreen(onDone: () -> Unit, onCancel: (() -> Unit)?) {
    val ctx = LocalContext.current
    val state by Core.pairing.collectAsStateWithLifecycle()
    var found by remember { mutableStateOf<List<DiscoveredPc>>(emptyList()) }
    var searching by remember { mutableStateOf(true) }
    var chosen by remember { mutableStateOf<DiscoveredPc?>(null) }
    var manualAddr by remember { mutableStateOf("") }
    var code by remember { mutableStateOf("") }
    var round by remember { mutableStateOf(0) }

    val scan = androidx.activity.compose.rememberLauncherForActivityResult(ScanContract()) { r ->
        r.contents?.let { uri ->
            Core.pairing.value = PairingUi.Waiting
            Core.start()
            Core.call { Core.node.pairWithQr(uri) } ?: run { Core.pairing.value = PairingUi.Failed("QR code non reconnu") }
        }
    }

    var blePerm by remember { mutableStateOf(BleScan.permitted(ctx)) }
    val bleAsk = androidx.activity.compose.rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        blePerm = ok
        if (ok) round++
    }
    LaunchedEffect(round) {
        searching = true
        found = coroutineScope {
            val wifi = async(Dispatchers.IO) {
                val lock = (ctx.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager).createMulticastLock("nc-pair")
                lock.acquire()
                try { discoverPcs(2500u) } finally { lock.release() }
            }
            // Bluetooth LE : utile quand le Wi-Fi bloque la découverte (réseaux d'entreprise, hotspots).
            val ble = async { BleScan.scan(ctx, 3000) }
            val viaWifi = wifi.await()
            val extra = ble.await().filter { b -> viaWifi.none { it.fingerprint == b.fingerprint } }
                .map { DiscoveredPc("PC trouvé en Bluetooth", it.fingerprint, 0u, listOf(it.addr)) }
            viaWifi + extra
        }
        searching = false
    }
    LaunchedEffect(state) {
        if (state is PairingUi.Done) {
            Core.pairing.value = PairingUi.Idle
            onDone()
        }
    }

    Column(
        Modifier.fillMaxSize().systemBarsPadding().verticalScroll(rememberScrollState()).padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Spacer(Modifier.height(12.dp))
        Text("Associer un PC", style = MaterialTheme.typography.headlineMedium)
        Text("Sur le PC, ouvrez NOTHING CONNECT et choisissez « Associer un téléphone ». Un code s'affiche.", color = Nc.Muted)

        when (val s = state) {
            PairingUi.Waiting -> Panel {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    CircularProgressIndicator(color = Nc.White, strokeWidth = 2.dp, modifier = Modifier.padding(4.dp))
                    Text("Confirmez l'association sur le PC.")
                }
            }
            is PairingUi.Failed -> Panel {
                Text("Association impossible", style = MaterialTheme.typography.titleMedium, color = Nc.Red)
                Text(s.reason, color = Nc.Muted)
                TextButton({ Core.pairing.value = PairingUi.Idle }) { Text("Réessayer") }
            }
            else -> {}
        }

        if (state !is PairingUi.Waiting) {
            Primary("Scanner le QR code", modifier = Modifier.fillMaxWidth()) {
                scan.launch(ScanOptions().setPrompt("Scannez le QR code affiché sur le PC").setBeepEnabled(false).setOrientationLocked(false))
            }

            Text("PC trouvés sur ce Wi-Fi", style = MaterialTheme.typography.titleMedium)
            if (searching) Text("Recherche…", color = Nc.Muted)
            else if (found.isEmpty()) Text("Aucun PC trouvé. Vérifiez que NOTHING CONNECT est ouvert sur le PC et que les deux appareils sont sur le même réseau.", color = Nc.Muted)
            found.forEach { pc ->
                Panel(Modifier.clickable { chosen = pc; manualAddr = pc.addrs.firstOrNull() ?: "" }) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        StatusDot(chosen == pc)
                        Text(pc.name, style = MaterialTheme.typography.titleMedium)
                    }
                    Text("Empreinte ${pc.fingerprint}", color = Nc.Muted)
                }
            }
            if (!searching) TextButton({ round++ }) { Text("Rechercher à nouveau") }
            if (android.os.Build.VERSION.SDK_INT >= 31 && !blePerm) {
                TextButton({ bleAsk.launch(android.Manifest.permission.BLUETOOTH_SCAN) }) { Text("Chercher aussi en Bluetooth") }
            }

            OutlinedTextField(
                value = manualAddr, onValueChange = { manualAddr = it.trim(); chosen = null },
                label = { Text("Adresse du PC (affichée sur le PC)") }, singleLine = true, modifier = Modifier.fillMaxWidth(),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
            )
            OutlinedTextField(
                value = code, onValueChange = { v -> code = v.filter { it.isDigit() }.take(6) },
                label = { Text("Code à 6 chiffres") }, singleLine = true, modifier = Modifier.fillMaxWidth(),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
            )
            if (code.isNotEmpty()) DotText(code.padEnd(6, '-').chunked(3).joinToString(" "), dot = 6.dp)
            if (manualAddr.isEmpty() && chosen == null) {
                Text("Sans adresse, l'association passe par le câble USB (débogage USB activé).", color = Nc.Muted)
            }
            Primary("Associer", enabled = code.length == 6 && (manualAddr.isEmpty() || manualAddr.contains(':')), modifier = Modifier.fillMaxWidth()) {
                Core.pairing.value = PairingUi.Waiting
                Core.start()
                // Toutes les adresses du PC trouvé (plusieurs cartes réseau), sinon l'adresse
                // saisie ; le câble USB est essayé en dernier recours.
                val addrs = chosen?.addrs?.takeIf { it.isNotEmpty() } ?: listOf(manualAddr).filter { it.isNotEmpty() }
                Core.call { Core.node.pairWithAddrs(addrs, code, chosen?.fingerprint) }
                    ?: run { Core.pairing.value = PairingUi.Failed("Adresse ou code invalide") }
            }
        }
        onCancel?.let { TextButton(it) { Text("Annuler") } }
    }
}
