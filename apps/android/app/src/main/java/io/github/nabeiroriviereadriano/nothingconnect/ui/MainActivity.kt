package io.github.nabeiroriviereadriano.nothingconnect.ui

import android.Manifest
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import io.github.nabeiroriviereadriano.nothingconnect.service.ConnectionService
import io.github.nabeiroriviereadriano.nothingconnect.share.OpenFiles
import uniffi.nc_ffi.NamedInput

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        if (Core.prefs.serviceEnabled && Core.devices.value.isNotEmpty()) ConnectionService.start(this)
        setContent { NcTheme { App() } }
    }
}

private enum class Tab(val label: String) { Home("Accueil"), Transfers("Transferts"), Settings("Réglages") }

@Composable
private fun App() {
    val ctx = LocalContext.current
    val devices by Core.devices.collectAsStateWithLifecycle()
    var tab by rememberSaveable { mutableStateOf(Tab.Home) }
    var pairing by rememberSaveable { mutableStateOf(false) }
    val snack = remember { SnackbarHostState() }
    val notifPerm = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {}

    LaunchedEffect(Unit) {
        if (Build.VERSION.SDK_INT >= 33) notifPerm.launch(Manifest.permission.POST_NOTIFICATIONS)
        Core.messages.collect { snack.showSnackbar(it) }
    }

    if (devices.isEmpty() || pairing) {
        PairScreen(onDone = {
            pairing = false
            Core.prefs.serviceEnabled = true
            ConnectionService.start(ctx)
        }, onCancel = if (devices.isEmpty()) null else ({ pairing = false }))
        return
    }

    Scaffold(
        containerColor = Nc.Black,
        snackbarHost = { SnackbarHost(snack) },
        bottomBar = {
            NavigationBar(containerColor = Nc.Black) {
                Tab.entries.forEach { t ->
                    NavigationBarItem(
                        selected = tab == t, onClick = { tab = t },
                        icon = { StatusDot(tab == t) }, label = { Text(t.label) },
                        colors = NavigationBarItemDefaults.colors(indicatorColor = Nc.Surface2, selectedTextColor = Nc.White, unselectedTextColor = Nc.Muted),
                    )
                }
            }
        },
    ) { pad ->
        Column(
            Modifier.fillMaxSize().padding(pad).verticalScroll(rememberScrollState()).padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            when (tab) {
                Tab.Home -> HomeScreen(onPair = { pairing = true })
                Tab.Transfers -> TransfersScreen()
                Tab.Settings -> SettingsScreen()
            }
        }
    }
}

@Composable
private fun HomeScreen(onPair: () -> Unit) {
    val ctx = LocalContext.current
    val devices by Core.devices.collectAsStateWithLifecycle()
    val connected by Core.connected.collectAsStateWithLifecycle()
    val refused by Core.refused.collectAsStateWithLifecycle()
    val online = devices.filter { it.id in connected.keys }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        val pc = online.firstOrNull() ?: return@rememberLauncherForActivityResult
        if (uris.isNotEmpty()) sendUris(ctx, pc.id, uris)
    }

    Spacer(Modifier.height(24.dp))
    DotText(if (online.isNotEmpty()) "${online.size}" else "0", dot = 9.dp)
    Text(if (online.isNotEmpty()) "PC connecté" else "Aucun PC connecté", style = androidx.compose.material3.MaterialTheme.typography.headlineMedium)
    Text(
        if (online.isNotEmpty()) "Les notifications, fichiers et commandes passent par votre Wi-Fi local."
        else "Ouvrez NOTHING CONNECT sur le PC. Les deux appareils doivent être sur le même Wi-Fi.",
        color = Nc.Muted,
    )
    val displayPeer by io.github.nabeiroriviereadriano.nothingconnect.stream.Streams.displayPeer.collectAsStateWithLifecycle()
    displayPeer?.let { id ->
        Panel {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                StatusDot(true)
                Text("Écran étendu de ${devices.firstOrNull { it.id == id }?.name ?: "votre PC"} ouvert", style = androidx.compose.material3.MaterialTheme.typography.titleMedium)
            }
            Text("Il est en pause tant que vous êtes ailleurs sur le téléphone.", color = Nc.Muted)
            Primary("Revenir à l'écran étendu") {
                ctx.startActivity(io.github.nabeiroriviereadriano.nothingconnect.stream.DisplayActivity.returnIntent(ctx))
            }
        }
    }
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Primary("Envoyer des fichiers", enabled = online.isNotEmpty()) { picker.launch(arrayOf("*/*")) }
        Secondary("Associer un PC") { onPair() }
    }
    devices.forEach { pc ->
        Panel {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                StatusDot(pc.id in connected.keys)
                Text(pc.name, style = androidx.compose.material3.MaterialTheme.typography.titleMedium)
            }
            Text(
                when {
                    pc.id in connected.keys && connected[pc.id]?.startsWith("127.0.0.1") == true -> "Connecté par câble USB"
                    pc.id in connected.keys -> "Connecté en Wi-Fi (${connected[pc.id]?.substringBefore(':')})"
                    pc.id in refused -> "Ce PC a révoqué l'association. Associez-le à nouveau."
                    else -> "Hors ligne, reconnexion automatique dès qu'il est joignable"
                },
                color = Nc.Muted,
            )
        }
    }
    StreamsPanel(online)
    Permissions()
}

/** Envoi depuis le sélecteur de l'appli (même mécanisme que le menu Partager). */
fun sendUris(ctx: android.content.Context, peerId: String, uris: List<Uri>) {
    val pfds = mutableListOf<ParcelFileDescriptor>()
    val inputs = uris.mapNotNull { uri ->
        val pfd = runCatching { ctx.contentResolver.openFileDescriptor(uri, "r") }.getOrNull() ?: return@mapNotNull null
        pfds += pfd
        val name = runCatching {
            ctx.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c -> if (c.moveToFirst()) c.getString(0) else null }
        }.getOrNull()
        NamedInput("/proc/self/fd/${pfd.fd}", name)
    }
    val id = Core.sendFiles(peerId, inputs)
    if (id == null) pfds.forEach { runCatching { it.close() } } else OpenFiles.hold(id, pfds)
}

@Composable
private fun TransfersScreen() {
    val incoming by Core.incoming.collectAsStateWithLifecycle()
    val transfers by Core.transfers.collectAsStateWithLifecycle()
    Text("Transferts", style = androidx.compose.material3.MaterialTheme.typography.headlineMedium)
    incoming.forEach { i ->
        Panel {
            Text("Le PC veut vous envoyer ${if (i.files.size == 1) i.files[0] else "${i.files.size} fichiers"}", style = androidx.compose.material3.MaterialTheme.typography.titleMedium)
            Text("${bytes(i.total)}, enregistrés dans Téléchargements/NOTHING CONNECT", color = Nc.Muted)
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Primary("Accepter") { Core.call { Core.node.acceptTransfer(i.id, true) } }
                Secondary("Refuser") { Core.call { Core.node.acceptTransfer(i.id, false) } }
            }
        }
    }
    if (transfers.isEmpty() && incoming.isEmpty()) {
        Text("Aucun transfert pendant cette session. Partagez un fichier vers « Envoyer au PC » depuis n'importe quelle application.", color = Nc.Muted)
    }
    transfers.values.sortedByDescending { it.state == "running" }.forEach { t ->
        Panel {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(if (t.outgoing) "Envoi" else "Réception", color = Nc.Muted, modifier = Modifier.padding(end = 10.dp))
                Text(t.file ?: "Transfert", maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            }
            LinearProgressIndicator(
                progress = { if (t.total > 0) t.done.toFloat() / t.total else 1f },
                modifier = Modifier.fillMaxWidth(), color = if (t.state == "interrupted") Nc.Red else Nc.White, trackColor = Nc.Line,
            )
            Text(
                "${bytes(t.done)} sur ${bytes(t.total)}" + (if (t.state == "running" && t.speedBps > 0) ", ${bytes(t.speedBps)}/s" + (t.etaS?.let { ", reste $it s" } ?: "") else "") + ". " + (STATE_LABEL[t.state] ?: t.state),
                color = Nc.Muted,
            )
            t.error?.let { Text(it, color = Nc.Red) }
            Row {
                if (t.state == "running") TextButton({ Core.call { Core.node.pauseTransfer(t.id) } }) { Text("Pause") }
                if (t.state == "paused") TextButton({ Core.call { Core.node.resumeTransfer(t.id) } }) { Text("Reprendre") }
                if (t.state !in setOf("completed", "failed", "cancelled", "rejected")) TextButton({ Core.call { Core.node.cancelTransfer(t.id) } }) { Text("Annuler", color = Nc.Red) }
            }
        }
    }
}
