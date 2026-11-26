package io.github.nabeiroriviereadriano.nothingconnect.share

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.core.content.IntentCompat
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import io.github.nabeiroriviereadriano.nothingconnect.core.PcDevice
import io.github.nabeiroriviereadriano.nothingconnect.service.ConnectionService
import io.github.nabeiroriviereadriano.nothingconnect.ui.NcTheme
import org.json.JSONObject
import uniffi.nc_ffi.NamedInput
import java.util.concurrent.ConcurrentHashMap

/** Menu « Partager » d'Android : fichiers, images, liens et textes vers un PC connecté. */
class ShareActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (Core.prefs.serviceEnabled) ConnectionService.start(this)
        val pcs = Core.devices.value.filter { it.id in Core.connected.value.keys }
        when {
            pcs.isEmpty() -> finishWith("Aucun PC connecté. Ouvrez NOTHING CONNECT sur le PC, sur le même Wi-Fi.")
            pcs.size == 1 -> { share(pcs[0]); finish() }
            else -> setContent {
                NcTheme {
                    AlertDialog(
                        onDismissRequest = { finish() },
                        title = { Text("Envoyer à") },
                        text = {
                            Column {
                                pcs.forEach { pc ->
                                    TextButton(onClick = { share(pc); finish() }, modifier = Modifier.padding(vertical = 2.dp)) { Text(pc.name) }
                                }
                            }
                        },
                        confirmButton = {},
                        dismissButton = { TextButton(onClick = { finish() }) { Text("Annuler") } },
                    )
                }
            }
        }
    }

    private fun finishWith(msg: String) {
        Toast.makeText(this, msg, Toast.LENGTH_LONG).show()
        finish()
    }

    private fun share(pc: PcDevice) {
        val uris: List<Uri> = when (intent.action) {
            Intent.ACTION_SEND -> listOfNotNull(IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java))
            Intent.ACTION_SEND_MULTIPLE -> IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java) ?: emptyList()
            else -> emptyList()
        }
        if (uris.isNotEmpty()) {
            sendUris(pc, uris)
            return
        }
        val text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()?.trim()
        if (text.isNullOrEmpty()) {
            Toast.makeText(this, "Rien à envoyer.", Toast.LENGTH_SHORT).show()
            return
        }
        val isLink = !text.contains(Regex("\\s")) && (text.startsWith("http://") || text.startsWith("https://"))
        Core.send(pc.id, JSONObject().put("t", "share").put("kind", if (isLink) "link" else "text").put("content", text))
        Toast.makeText(this, "Envoyé à ${pc.name}", Toast.LENGTH_SHORT).show()
    }

    /**
     * Les contenus `content://` sont ouverts ici et transmis au cœur sous la forme
     * `/proc/self/fd/N` : aucun fichier n'est copié, même pour une vidéo de plusieurs Go.
     * Les descripteurs restent ouverts jusqu'à la fin du transfert.
     */
    private fun sendUris(pc: PcDevice, uris: List<Uri>) {
        val pfds = mutableListOf<ParcelFileDescriptor>()
        val inputs = uris.mapNotNull { uri ->
            val pfd = runCatching { contentResolver.openFileDescriptor(uri, "r") }.getOrNull() ?: return@mapNotNull null
            pfds += pfd
            NamedInput(path = "/proc/self/fd/${pfd.fd}", name = displayName(uri))
        }
        if (inputs.isEmpty()) {
            Toast.makeText(this, "Impossible de lire ces fichiers.", Toast.LENGTH_LONG).show()
            return
        }
        val id = Core.sendFiles(pc.id, inputs)
        if (id == null) {
            pfds.forEach { runCatching { it.close() } }
        } else {
            OpenFiles.hold(id, pfds)
            Toast.makeText(this, "Envoi vers ${pc.name}…", Toast.LENGTH_SHORT).show()
        }
    }

    private fun displayName(uri: Uri): String? = runCatching {
        contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
            if (c.moveToFirst()) c.getString(0) else null
        }
    }.getOrNull() ?: uri.lastPathSegment
}

/** Descripteurs ouverts pour les envois en cours ; fermés à la fin du transfert. */
object OpenFiles {
    private val open = ConcurrentHashMap<String, List<ParcelFileDescriptor>>()
    private var subscribed = false

    @Synchronized
    fun hold(transferId: String, pfds: List<ParcelFileDescriptor>) {
        open[transferId] = pfds
        if (!subscribed) {
            subscribed = true
            Core.subscribe { e ->
                if (e.getString("type") == "transfer_finished") {
                    open.remove(e.getString("id"))?.forEach { runCatching { it.close() } }
                }
            }
        }
    }
}
