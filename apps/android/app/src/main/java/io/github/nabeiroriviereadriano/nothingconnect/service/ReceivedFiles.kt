package io.github.nabeiroriviereadriano.nothingconnect.service

import android.content.ContentValues
import android.content.Context
import android.provider.MediaStore
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import org.json.JSONObject
import java.io.File

/**
 * Le cœur écrit et vérifie (SHA-256) chaque fichier dans le dossier privé de l'appli.
 * Une fois vérifié, il est publié dans Téléchargements/NOTHING CONNECT via MediaStore
 * (stockage partagé, aucune permission requise) en conservant l'arborescence.
 */
class ReceivedFiles(private val ctx: Context) {
    fun onFileDone(e: JSONObject) {
        if (e.isNull("path")) return // fichier envoyé, pas reçu
        val outcome = e.getString("outcome")
        val file = File(e.getString("path"))
        if (outcome != "saved" || !file.exists()) return
        Thread { publish(file) }.start()
    }

    private fun publish(file: File) {
        val base = Core.receivedDir.canonicalFile
        val rel = file.canonicalFile.relativeTo(base).parent?.replace(File.separatorChar, '/')
        val relPath = "Download/NOTHING CONNECT" + (rel?.let { "/$it" } ?: "")
        val values = ContentValues().apply {
            put(MediaStore.Downloads.DISPLAY_NAME, file.name)
            put(MediaStore.Downloads.RELATIVE_PATH, relPath)
            put(MediaStore.Downloads.IS_PENDING, 1)
        }
        val resolver = ctx.contentResolver
        val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values) ?: return
        try {
            resolver.openOutputStream(uri)!!.use { out -> file.inputStream().use { it.copyTo(out, 1 shl 20) } }
            resolver.update(uri, ContentValues().apply { put(MediaStore.Downloads.IS_PENDING, 0) }, null, null)
            file.delete()
        } catch (ex: Exception) {
            resolver.delete(uri, null, null)
            Core.messages.tryEmit("Impossible d'enregistrer ${file.name} dans Téléchargements : ${ex.message}. Le fichier reste dans le dossier de l'application.")
        }
    }
}
