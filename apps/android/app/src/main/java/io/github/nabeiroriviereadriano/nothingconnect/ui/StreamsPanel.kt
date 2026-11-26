package io.github.nabeiroriviereadriano.nothingconnect.ui

import android.content.Intent
import android.provider.Settings
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import io.github.nabeiroriviereadriano.nothingconnect.core.PcDevice
import io.github.nabeiroriviereadriano.nothingconnect.stream.ClipboardSendActivity
import io.github.nabeiroriviereadriano.nothingconnect.stream.ControlService
import io.github.nabeiroriviereadriano.nothingconnect.stream.Streams

/** Écran, son et presse-papiers : ce qui se règle côté téléphone. */
@Composable
fun StreamsPanel(online: List<PcDevice>) {
    val ctx = LocalContext.current
    val pcAudio by Core.pcAudioOn.collectAsStateWithLifecycle()
    var tick by remember { mutableIntStateOf(0) }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(Unit) { lifecycle.repeatOnLifecycle(Lifecycle.State.RESUMED) { tick++ } }
    val control = remember(tick) { ControlService.instance != null }

    Panel {
        Text("Écran, son et presse-papiers", style = MaterialTheme.typography.titleMedium)
        Text(
            "Depuis le PC : page Miroir pour voir ce téléphone (et l'entendre), page Écran étendu pour en faire un second écran. Le téléphone demande votre accord à chaque diffusion.",
            color = Nc.Muted,
        )
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            StatusDot(control)
            Text(if (control) "Contrôle depuis le PC activé" else "Contrôle depuis le PC désactivé")
        }
        if (!control) {
            Text(
                "Pour toucher et taper sur le téléphone depuis le PC, activez « NOTHING CONNECT : contrôle depuis le PC » dans Accessibilité. " +
                    "Si l'option est grisée : Paramètres › Applications › NOTHING CONNECT › menu ⋮ › Autoriser les paramètres restreints.",
                color = Nc.Muted,
            )
            Secondary("Ouvrir Accessibilité") { ctx.startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)) }
        }
        SwitchRow("Son du PC sur ce téléphone", "Le son de Windows sort par le téléphone (écouteurs, enceinte).", pcAudio) { on ->
            val pc = online.firstOrNull()
            if (on && pc != null) Streams.requestPcAudio(pc.id) else Streams.stopPcAudio()
        }
        Secondary("Envoyer le presse-papiers au PC", enabled = online.isNotEmpty()) {
            ctx.startActivity(Intent(ctx, ClipboardSendActivity::class.java))
        }
        Text("Astuce : ajoutez la tuile « Presse-papiers → PC » aux réglages rapides.", color = Nc.Muted, style = MaterialTheme.typography.bodyMedium)
    }
}
