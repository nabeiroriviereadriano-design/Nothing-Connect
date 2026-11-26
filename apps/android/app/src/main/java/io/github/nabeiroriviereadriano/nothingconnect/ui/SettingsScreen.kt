package io.github.nabeiroriviereadriano.nothingconnect.ui

import android.annotation.SuppressLint
import android.app.NotificationManager
import android.content.ComponentName
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.app.NotificationManagerCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import io.github.nabeiroriviereadriano.nothingconnect.BuildConfig
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import io.github.nabeiroriviereadriano.nothingconnect.core.Updater
import io.github.nabeiroriviereadriano.nothingconnect.notif.NotifListener
import io.github.nabeiroriviereadriano.nothingconnect.service.ConnectionService
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable
fun SwitchRow(title: String, hint: String?, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(title)
            hint?.let { Text(it, color = Nc.Muted, style = MaterialTheme.typography.bodyMedium) }
        }
        Switch(checked, onChange, colors = SwitchDefaults.colors(checkedTrackColor = Nc.White, checkedThumbColor = Nc.Black, uncheckedTrackColor = Nc.Black, uncheckedBorderColor = Nc.Line))
    }
}

/** Autorisations nécessaires, chacune expliquée ; affichées tant qu'elles manquent. */
@SuppressLint("BatteryLife")
@Composable
fun Permissions() {
    val ctx = LocalContext.current
    var tick by remember { mutableIntStateOf(0) }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(Unit) { lifecycle.repeatOnLifecycle(Lifecycle.State.RESUMED) { tick++ } }
    // Réévalué à chaque retour sur l'écran (l'utilisateur revient des réglages Android).
    val (notifAccess, dnd, battery) = remember(tick) {
        Triple(
            NotificationManagerCompat.getEnabledListenerPackages(ctx).contains(ctx.packageName),
            ctx.getSystemService(NotificationManager::class.java).isNotificationPolicyAccessGranted,
            ctx.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(ctx.packageName),
        )
    }
    if (notifAccess && dnd && battery) return
    Panel {
        Text("Autorisations", style = MaterialTheme.typography.titleMedium)
        if (!notifAccess) {
            Text("Accès aux notifications : pour afficher vos notifications et contrôler la musique depuis le PC.", color = Nc.Muted)
            Secondary("Autoriser l'accès") {
                val intent = if (Build.VERSION.SDK_INT >= 30) {
                    Intent(Settings.ACTION_NOTIFICATION_LISTENER_DETAIL_SETTINGS).putExtra(
                        Settings.EXTRA_NOTIFICATION_LISTENER_COMPONENT_NAME, ComponentName(ctx, NotifListener::class.java).flattenToString())
                } else {
                    Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS)
                }
                ctx.startActivity(intent)
            }
        }
        if (!battery) {
            Text("Sans restriction de batterie : pour que la connexion tienne quand le téléphone est en veille. Android peut sinon couper l'application.", color = Nc.Muted)
            Secondary("Ne pas restreindre") {
                ctx.startActivity(Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:${ctx.packageName}")))
            }
        }
        if (!dnd) {
            Text("Accès « Ne pas déranger » (facultatif) : pour l'activer depuis le PC.", color = Nc.Muted)
            Secondary("Autoriser") { ctx.startActivity(Intent(Settings.ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS)) }
        }
    }
}

@Composable
fun SettingsScreen() {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val devices by Core.devices.collectAsStateWithLifecycle()
    var service by remember { mutableStateOf(Core.prefs.serviceEnabled) }
    var shareNotifs by remember { mutableStateOf(Core.prefs.shareNotifications) }
    var clip by remember { mutableStateOf(Core.prefs.receiveClipboard) }
    var updateText by remember { mutableStateOf<String?>(null) }
    var release by remember { mutableStateOf<Updater.Release?>(null) }
    var progress by remember { mutableFloatStateOf(-1f) }
    var removing by remember { mutableStateOf<String?>(null) }

    Text("Réglages", style = MaterialTheme.typography.headlineMedium)
    Panel {
        SwitchRow("Rester connecté en arrière-plan", "Notification permanente discrète tant que c'est activé.", service) {
            service = it
            Core.prefs.serviceEnabled = it
            if (it) ConnectionService.start(ctx) else ConnectionService.stop(ctx)
        }
        SwitchRow("Envoyer les notifications au PC", "Le choix des applications se fait depuis le PC.", shareNotifs) {
            shareNotifs = it
            Core.prefs.shareNotifications = it
        }
        SwitchRow("Recevoir le presse-papiers du PC", "Quand le partage du presse-papiers est activé sur le PC, le texte copié sur le PC est copié ici.", clip) {
            clip = it
            Core.prefs.receiveClipboard = it
        }
    }
    Panel {
        Text("PC associés", style = MaterialTheme.typography.titleMedium)
        devices.forEach { pc ->
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(pc.name)
                    Text("Version ${pc.appVersion ?: "inconnue"}", color = Nc.Muted, style = MaterialTheme.typography.bodyMedium)
                }
                TextButton({ removing = pc.id }) { Text("Supprimer", color = Nc.Red) }
            }
        }
        Text("Empreinte de ce téléphone : ${Core.node.fingerprint()}", color = Nc.Muted, style = MaterialTheme.typography.bodyMedium)
    }
    Panel {
        Text("Mises à jour", style = MaterialTheme.typography.titleMedium)
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Text("Version installée", color = Nc.Muted)
            DotText(BuildConfig.VERSION_NAME.replace('.', ' '), dot = 3.dp)
        }
        updateText?.let { Text(it, color = Nc.Muted) }
        if (progress >= 0f) LinearProgressIndicator(progress = { progress }, modifier = Modifier.fillMaxWidth(), color = Nc.White, trackColor = Nc.Line)
        val rel = release
        if (rel == null) {
            Secondary("Rechercher une mise à jour") {
                updateText = "Recherche…"
                scope.launch {
                    val r = withContext(Dispatchers.IO) { runCatching { Updater.check() } }
                    release = r.getOrNull()
                    updateText = when {
                        r.isFailure -> "Vérification impossible : ${r.exceptionOrNull()?.message}"
                        release == null -> "Vous avez la dernière version."
                        else -> "Version ${release!!.version} disponible. Vos PC associés et réglages sont conservés."
                    }
                }
            }
        } else {
            Primary("Installer la version ${rel.version}", enabled = progress < 0f) {
                progress = 0f
                scope.launch {
                    val r = withContext(Dispatchers.IO) { runCatching { Updater.downloadAndInstall(ctx, rel) { p -> progress = p } } }
                    progress = -1f
                    r.exceptionOrNull()?.let { updateText = it.message }
                }
            }
        }
    }
    removing?.let { id ->
        AlertDialog(
            onDismissRequest = { removing = null },
            title = { Text("Supprimer ce PC ?") },
            text = { Text("Les clés sont révoquées des deux côtés. Il faudra l'associer à nouveau.") },
            confirmButton = { TextButton({ Core.call { Core.node.removeDevice(id) }; removing = null }) { Text("Supprimer", color = Nc.Red) } },
            dismissButton = { TextButton({ removing = null }) { Text("Annuler") } },
        )
    }
}
