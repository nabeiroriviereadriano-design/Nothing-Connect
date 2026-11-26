package io.github.nabeiroriviereadriano.nothingconnect.stream

import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import io.github.nabeiroriviereadriano.nothingconnect.R
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import io.github.nabeiroriviereadriano.nothingconnect.core.Notifs
import org.json.JSONObject

/** Numéros des flux (identiques au protocole). */
object Kind {
    const val MIRROR = 1
    const val PHONE_AUDIO = 2
    const val PC_AUDIO = 3
    const val DISPLAY = 4
}

/** Drapeaux des fragments de flux. */
object Flag {
    const val CONFIG = 1
    const val KEYFRAME = 2
}

fun JSONObject.intOrNull(k: String): Int? = if (has(k) && !isNull(k)) optInt(k) else null
fun JSONObject.boolOrNull(k: String): Boolean? = if (has(k) && !isNull(k)) optBoolean(k) else null

/**
 * Aiguillage des flux temps réel : demandes du PC (miroir, son, écran étendu, contrôle)
 * et paquets reçus (son du PC, images de l'écran étendu).
 */
object Streams {
    const val NOTIF_REQUEST = 40
    private var pcAudio: PcAudioPlayer? = null
    private var pcAudioPeer: String? = null
    private lateinit var app: Context

    fun init(ctx: Context) {
        app = ctx.applicationContext
        Core.onStreamPacket { peer, kind, flags, ts, data ->
            when (kind) {
                Kind.PC_AUDIO -> if (peer == pcAudioPeer) pcAudio?.push(data)
                Kind.DISPLAY -> DisplayActivity.onFrame(peer, flags, ts, data)
            }
        }
    }

    fun stopMessage(kind: String, reason: String? = null): JSONObject =
        JSONObject().put("t", "stream_stop").put("kind", kind).put("reason", reason ?: JSONObject.NULL)

    fun startMessage(kind: String, params: JSONObject): JSONObject =
        JSONObject().put("t", "stream_start").put("kind", kind).put("params", params)

    /** Appelé (en série) pour chaque demande du PC qui concerne les flux. Vrai si traitée. */
    fun handle(peer: String, msg: JSONObject): Boolean {
        val kind = msg.optString("kind")
        when (msg.getString("t")) {
            "stream_request" -> {
                val params = msg.optJSONObject("params") ?: JSONObject()
                when (kind) {
                    "mirror", "phone_audio" -> ProjectionActivity.request(app, peer, kind, params)
                    "display" -> DisplayActivity.request(app, peer, params)
                    // Son du PC : c'est le PC qui le capture ; demande lancée depuis le téléphone.
                    else -> Core.send(peer, stopMessage(kind, "Flux non pris en charge par le téléphone"))
                }
            }
            "stream_stop" -> when (kind) {
                "mirror", "phone_audio" -> MirrorService.stopFromPc(app, kind)
                "display" -> DisplayActivity.stopFromPc()
                "pc_audio" -> stopPcAudio(sendStop = false)
            }
            "stream_keyframe" -> if (kind == "mirror") MirrorService.instance?.requestKeyframe()
            "stream_start" -> when (kind) {
                "pc_audio" -> startPcAudioPlayer(peer)
                // Le PC confirme le format de l'écran étendu (H.264 ou JPEG, définition réelle).
                "display" -> DisplayActivity.onPcStart(peer, msg.optJSONObject("params") ?: JSONObject())
            }
            "input" -> {
                val c = ControlService.instance
                if (c == null) {
                    Core.send(peer, JSONObject().put("t", "capabilities").put("capabilities", Core.capabilities()))
                } else {
                    c.handle(msg)
                }
            }
            else -> return false
        }
        return true
    }

    fun onPeerGone(peer: String) {
        if (pcAudioPeer == peer) stopPcAudio(sendStop = false)
        MirrorService.instance?.stopIfPeer(peer)
        DisplayActivity.stopIfPeer(peer)
    }

    // ------------------------------------------------------------------ son du PC

    val pcAudioActive: Boolean get() = pcAudio != null

    /** PC dont l'écran étendu est ouvert sur ce téléphone (même en arrière-plan), sinon null. */
    val displayPeer = kotlinx.coroutines.flow.MutableStateFlow<String?>(null)

    /** Demande au PC de diffuser son son vers ce téléphone. */
    fun requestPcAudio(peer: String) {
        Core.send(peer, JSONObject().put("t", "stream_request").put("kind", "pc_audio").put("params", JSONObject()))
    }

    @Synchronized
    private fun startPcAudioPlayer(peer: String) {
        pcAudio?.stop()
        pcAudio = runCatching { PcAudioPlayer() }.getOrNull()
        pcAudioPeer = if (pcAudio != null) peer else null
        if (pcAudio == null) Core.send(peer, stopMessage("pc_audio", "Lecture du son impossible sur le téléphone"))
        Core.pcAudioOn.value = pcAudio != null
    }

    @Synchronized
    fun stopPcAudio(sendStop: Boolean = true) {
        val peer = pcAudioPeer
        pcAudio?.stop()
        pcAudio = null
        pcAudioPeer = null
        Core.pcAudioOn.value = false
        if (sendStop && peer != null) Core.send(peer, stopMessage("pc_audio"))
    }

    // ------------------------------------------------------------------ ouverture d'écrans

    /**
     * Ouvre une activité à la demande du PC. Android bloque souvent l'ouverture depuis
     * l'arrière-plan : une notification permet alors de l'ouvrir d'un toucher. L'activité
     * retire la notification dès qu'elle s'affiche.
     */
    fun openOrNotify(ctx: Context, intent: Intent, title: String, text: String) {
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        val pi = PendingIntent.getActivity(ctx, NOTIF_REQUEST, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val n = NotificationCompat.Builder(ctx, Notifs.CH_INCOMING)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle(title)
            .setContentText(text)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setCategory(NotificationCompat.CATEGORY_CALL)
            .setAutoCancel(true)
            .setTimeoutAfter(60_000)
            .setContentIntent(pi)
            .build()
        runCatching { ctx.getSystemService(NotificationManager::class.java).notify(NOTIF_REQUEST, n) }
        runCatching { ctx.startActivity(intent) }
    }

    fun clearRequestNotification(ctx: Context) {
        ctx.getSystemService(NotificationManager::class.java).cancel(NOTIF_REQUEST)
    }
}
