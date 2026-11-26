package io.github.nabeiroriviereadriano.nothingconnect.core

import android.content.Context
import android.content.Intent
import androidx.core.content.FileProvider
import io.github.nabeiroriviereadriano.nothingconnect.BuildConfig
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

/**
 * Mises à jour depuis GitHub Releases. L'APK téléchargé est vérifié deux fois :
 * empreinte SHA-256 publiée avec la version, puis signature par Android (même clé
 * obligatoire pour installer par-dessus : données et associations conservées).
 */
object Updater {
    private const val API = "https://api.github.com/repos/nabeiroriviereadriano-design/Nothing-Connect/releases/latest"

    data class Release(val version: String, val notes: String, val apkUrl: String, val sumsUrl: String?)

    private fun get(url: String): HttpURLConnection = (URL(url).openConnection() as HttpURLConnection).apply {
        connectTimeout = 10_000
        readTimeout = 30_000
        setRequestProperty("Accept", "application/vnd.github+json")
        setRequestProperty("User-Agent", "NothingConnect/${BuildConfig.VERSION_NAME}")
    }

    /** Version plus récente disponible, ou null. Bloquant : appeler hors du thread principal. */
    fun check(): Release? {
        val json = get(API).inputStream.bufferedReader().use { JSONObject(it.readText()) }
        val version = json.getString("tag_name").removePrefix("v")
        if (!newer(version, BuildConfig.VERSION_NAME)) return null
        val assets = json.getJSONArray("assets")
        var apk: String? = null
        var sums: String? = null
        for (i in 0 until assets.length()) {
            val a = assets.getJSONObject(i)
            val name = a.getString("name")
            if (name.endsWith(".apk")) apk = a.getString("browser_download_url")
            if (name == "SHA256SUMS.txt") sums = a.getString("browser_download_url")
        }
        return apk?.let { Release(version, json.optString("body"), it, sums) }
    }

    fun newer(remote: String, local: String): Boolean {
        val r = remote.split(".").map { it.takeWhile(Char::isDigit).toIntOrNull() ?: 0 }
        val l = local.split(".").map { it.takeWhile(Char::isDigit).toIntOrNull() ?: 0 }
        for (i in 0 until maxOf(r.size, l.size)) {
            val a = r.getOrElse(i) { 0 }
            val b = l.getOrElse(i) { 0 }
            if (a != b) return a > b
        }
        return false
    }

    /** Télécharge, vérifie l'empreinte et lance l'installateur Android. */
    fun downloadAndInstall(ctx: Context, rel: Release, progress: (Float) -> Unit) {
        val dir = File(ctx.cacheDir, "updates").apply { mkdirs(); listFiles()?.forEach { it.delete() } }
        val apk = File(dir, "NothingConnect-${rel.version}.apk")
        val conn = get(rel.apkUrl)
        val total = conn.contentLengthLong
        val md = MessageDigest.getInstance("SHA-256")
        conn.inputStream.use { input ->
            apk.outputStream().use { out ->
                val buf = ByteArray(1 shl 16)
                var done = 0L
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) break
                    out.write(buf, 0, n)
                    md.update(buf, 0, n)
                    done += n
                    if (total > 0) progress(done.toFloat() / total)
                }
            }
        }
        val digest = md.digest().joinToString("") { "%02x".format(it) }
        val expected = rel.sumsUrl?.let { url ->
            get(url).inputStream.bufferedReader().readLines()
                .map { it.trim().split(Regex("\\s+")) }
                .firstOrNull { it.size >= 2 && it.last().trimStart('*').endsWith(".apk") }?.first()
        } ?: error("Empreinte de la mise à jour introuvable : installation annulée.")
        if (!expected.equals(digest, ignoreCase = true)) {
            apk.delete()
            error("Empreinte différente : le fichier téléchargé est corrompu. Installation annulée.")
        }
        val uri = FileProvider.getUriForFile(ctx, "${ctx.packageName}.files", apk)
        ctx.startActivity(
            Intent(Intent.ACTION_VIEW).setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        )
    }
}
