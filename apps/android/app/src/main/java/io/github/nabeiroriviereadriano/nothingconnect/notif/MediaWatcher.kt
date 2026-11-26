package io.github.nabeiroriviereadriano.nothingconnect.notif

import android.content.ComponentName
import android.content.Context
import android.graphics.Bitmap
import android.media.MediaMetadata
import android.media.session.MediaController
import android.media.session.MediaSessionManager
import android.media.session.PlaybackState
import android.os.Handler
import android.os.Looper
import android.util.Base64
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream

/**
 * Sessions multimédias actives (Spotify, YouTube Music, Deezer…) via MediaSessionManager.
 * Exige l'accès aux notifications. Seules les applis qui publient une MediaSession sont visibles.
 */
object MediaWatcher {
    private val main = Handler(Looper.getMainLooper())
    private var manager: MediaSessionManager? = null
    private var controllers: List<MediaController> = emptyList()
    private val artCache = HashMap<String, String>()
    private var lastJson = ""

    private val callback = object : MediaController.Callback() {
        override fun onPlaybackStateChanged(state: PlaybackState?) = schedule()
        override fun onMetadataChanged(metadata: MediaMetadata?) = schedule()
        override fun onSessionDestroyed() = schedule()
    }
    private val sessionsListener = MediaSessionManager.OnActiveSessionsChangedListener { list -> attach(list ?: emptyList()) }
    private val publishRunnable = Runnable { publish() }

    fun ensure(ctx: Context) {
        main.post {
            if (manager != null) return@post
            val m = ctx.getSystemService(MediaSessionManager::class.java)
            val component = ComponentName(ctx, NotifListener::class.java)
            try {
                m.addOnActiveSessionsChangedListener(sessionsListener, component, main)
                manager = m
                attach(m.getActiveSessions(component))
            } catch (_: SecurityException) {
                // Accès aux notifications pas encore accordé : réessayé à la connexion du listener.
            }
        }
    }

    private fun attach(list: List<MediaController>) {
        controllers.forEach { runCatching { it.unregisterCallback(callback) } }
        controllers = list
        list.forEach { it.registerCallback(callback, main) }
        schedule()
    }

    private fun schedule() {
        main.removeCallbacks(publishRunnable)
        main.postDelayed(publishRunnable, 300)
    }

    fun publish() {
        main.post {
            val arr = JSONArray()
            controllers.forEach { c -> arr.put(toJson(c)) }
            val json = JSONObject().put("t", "media_sessions").put("sessions", arr)
            val s = json.toString()
            if (s != lastJson || Core.connected.value.isNotEmpty()) {
                lastJson = s
                Core.broadcast(json)
            }
        }
    }

    private fun toJson(c: MediaController): JSONObject {
        val md = c.metadata
        val st = c.playbackState
        val actions = st?.actions ?: 0L
        val state = when (st?.state) {
            PlaybackState.STATE_PLAYING -> "playing"
            PlaybackState.STATE_PAUSED -> "paused"
            PlaybackState.STATE_STOPPED, PlaybackState.STATE_NONE -> "stopped"
            PlaybackState.STATE_BUFFERING, PlaybackState.STATE_CONNECTING -> "buffering"
            else -> "other"
        }
        val label = runCatching {
            val pm = Core.app.packageManager
            pm.getApplicationLabel(pm.getApplicationInfo(c.packageName, 0)).toString()
        }.getOrDefault(c.packageName)
        return JSONObject()
            .put("id", c.packageName)
            .put("package", c.packageName)
            .put("app_name", label)
            .put("title", md?.getString(MediaMetadata.METADATA_KEY_TITLE) ?: JSONObject.NULL)
            .put("artist", md?.getString(MediaMetadata.METADATA_KEY_ARTIST) ?: JSONObject.NULL)
            .put("album", md?.getString(MediaMetadata.METADATA_KEY_ALBUM) ?: JSONObject.NULL)
            .put("duration_ms", md?.getLong(MediaMetadata.METADATA_KEY_DURATION)?.takeIf { it > 0 } ?: JSONObject.NULL)
            .put("position_ms", st?.position?.takeIf { it >= 0 } ?: JSONObject.NULL)
            .put("state", state)
            .put("artwork_jpeg_b64", artwork(c.packageName, md) ?: JSONObject.NULL)
            .put("can_skip_next", actions and PlaybackState.ACTION_SKIP_TO_NEXT != 0L)
            .put("can_skip_prev", actions and PlaybackState.ACTION_SKIP_TO_PREVIOUS != 0L)
    }

    /** Pochette réduite à 160 px en JPEG (≈ 8 Ko), recalculée seulement si le titre change. */
    private fun artwork(pkg: String, md: MediaMetadata?): String? {
        md ?: return null
        val key = pkg + "|" + md.getString(MediaMetadata.METADATA_KEY_TITLE)
        artCache[key]?.let { return it }
        val bmp = md.getBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART) ?: md.getBitmap(MediaMetadata.METADATA_KEY_ART) ?: return null
        val scaled = Bitmap.createScaledBitmap(bmp, 160, 160 * bmp.height / bmp.width.coerceAtLeast(1), true)
        val out = ByteArrayOutputStream()
        scaled.compress(Bitmap.CompressFormat.JPEG, 80, out)
        val b64 = Base64.encodeToString(out.toByteArray(), Base64.NO_WRAP)
        if (artCache.size > 20) artCache.clear()
        artCache[key] = b64
        return b64
    }

    fun command(sessionId: String, action: String) {
        main.post {
            val c = controllers.firstOrNull { it.packageName == sessionId } ?: return@post
            val t = c.transportControls
            when (action) {
                "play" -> t.play()
                "pause" -> t.pause()
                "toggle_play_pause" -> if (c.playbackState?.state == PlaybackState.STATE_PLAYING) t.pause() else t.play()
                "next" -> t.skipToNext()
                "previous" -> t.skipToPrevious()
            }
        }
    }
}
