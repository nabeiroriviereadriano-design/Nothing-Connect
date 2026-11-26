package io.github.nabeiroriviereadriano.nothingconnect.service

import android.app.NotificationManager
import android.content.Context
import android.media.AudioAttributes
import android.media.AudioManager
import android.media.Ringtone
import android.media.RingtoneManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import io.github.nabeiroriviereadriano.nothingconnect.notif.MediaWatcher
import io.github.nabeiroriviereadriano.nothingconnect.notif.NotifListener
import org.json.JSONObject

/** Exécute les demandes du PC et répond avec le résultat réel (jamais de succès simulé). */
class Commands(private val ctx: Context) {
    private val main = Handler(Looper.getMainLooper())
    private var ringtone: Ringtone? = null

    fun handle(peer: String, msg: JSONObject) {
        when (msg.getString("t")) {
            "command" -> {
                val id = msg.getLong("id")
                val result = runCatching { run(msg.getJSONObject("command")) }
                val reply = JSONObject().put("t", "command_result").put("id", id)
                    .put("ok", result.isSuccess)
                    .put("error", result.exceptionOrNull()?.message ?: JSONObject.NULL)
                Core.send(peer, reply)
            }
            "notification_action" -> NotifListener.instance?.perform(
                msg.getString("key"), msg.getInt("action_index"), msg.optString("reply").takeIf { msg.has("reply") && !msg.isNull("reply") },
            )
            "notification_dismiss" -> NotifListener.instance?.dismiss(msg.getString("key"))
            "notification_open" -> NotifListener.instance?.open(msg.getString("key"))
            "notification_filter" -> {
                Core.prefs.setAppBlocked(msg.getString("package"), !msg.getBoolean("enabled"))
                NotifListener.instance?.sendAppList()
            }
            "media_command" -> MediaWatcher.command(msg.getString("session_id"), msg.getString("action"))
        }
    }

    private fun run(c: JSONObject) {
        when (c.getString("c")) {
            "ring" -> main.post { ring() }
            "stop_ring" -> main.post { stopRing() }
            "vibrate" -> {
                val ms = c.optLong("millis", 500).coerceIn(50, 5000)
                val vibrator = if (Build.VERSION.SDK_INT >= 31) {
                    ctx.getSystemService(VibratorManager::class.java).defaultVibrator
                } else {
                    @Suppress("DEPRECATION") ctx.getSystemService(Vibrator::class.java)
                }
                vibrator.vibrate(VibrationEffect.createOneShot(ms, VibrationEffect.DEFAULT_AMPLITUDE))
            }
            "set_media_volume" -> {
                val am = ctx.getSystemService(AudioManager::class.java)
                val max = am.getStreamMaxVolume(AudioManager.STREAM_MUSIC)
                am.setStreamVolume(AudioManager.STREAM_MUSIC, c.getInt("percent").coerceIn(0, 100) * max / 100, 0)
            }
            "set_do_not_disturb" -> {
                val nm = ctx.getSystemService(NotificationManager::class.java)
                if (!nm.isNotificationPolicyAccessGranted) error("Accès « Ne pas déranger » non autorisé sur le téléphone")
                nm.setInterruptionFilter(
                    if (c.getBoolean("enabled")) NotificationManager.INTERRUPTION_FILTER_PRIORITY else NotificationManager.INTERRUPTION_FILTER_ALL
                )
            }
            "refresh_status" -> {}
            else -> error("Commande non prise en charge")
        }
    }

    /** Sonne fort (flux alarme) même en mode silencieux, 30 s maximum : pour retrouver le téléphone. */
    private fun ring() {
        stopRing()
        val uri = RingtoneManager.getActualDefaultRingtoneUri(ctx, RingtoneManager.TYPE_RINGTONE)
            ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM)
        ringtone = RingtoneManager.getRingtone(ctx, uri)?.apply {
            audioAttributes = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ALARM).build()
            isLooping = true
            play()
        }
        main.postDelayed({ stopRing() }, 30_000)
    }

    fun stopRing() {
        ringtone?.stop()
        ringtone = null
    }
}
