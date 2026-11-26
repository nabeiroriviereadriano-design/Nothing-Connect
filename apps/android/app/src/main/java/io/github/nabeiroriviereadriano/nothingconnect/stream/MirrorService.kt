package io.github.nabeiroriviereadriano.nothingconnect.stream

import android.annotation.SuppressLint
import android.app.Notification
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioPlaybackCaptureConfiguration
import android.media.AudioRecord
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.os.IBinder
import android.os.SystemClock
import android.util.Log
import android.view.Display
import android.view.Surface
import android.view.WindowManager
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import io.github.nabeiroriviereadriano.nothingconnect.R
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import io.github.nabeiroriviereadriano.nothingconnect.core.Notifs
import org.json.JSONObject
import kotlin.math.max
import kotlin.math.roundToInt

/**
 * Diffusion de l'écran (H.264 matériel via MediaCodec) et/ou du son du téléphone
 * (AudioPlaybackCapture, PCM 48 kHz stéréo) vers un PC, après accord de l'utilisateur.
 */
class MirrorService : Service() {
    private var projection: MediaProjection? = null
    private var display: VirtualDisplay? = null
    @Volatile private var codec: MediaCodec? = null
    private var inputSurface: Surface? = null
    private var record: AudioRecord? = null
    @Volatile private var running = false
    private lateinit var peer: String
    private var video = false
    private var audio = false
    private var maxSize = 1600
    private var bitrate = 8_000_000
    private var fps = 60
    private var width = 0
    private var height = 0
    @Volatile private var lastKeyRequest = 0L
    private val thread = HandlerThread("nc-mirror").apply { start() }
    private val handler = Handler(thread.looper)

    private val displayListener = object : DisplayManager.DisplayListener {
        override fun onDisplayAdded(id: Int) {}
        override fun onDisplayRemoved(id: Int) {}
        override fun onDisplayChanged(id: Int) {
            if (id != Display.DEFAULT_DISPLAY || !running || !video) return
            val (w, h, _) = targetSize()
            // Rotation : on recrée l'encodeur à la nouvelle taille (même affichage virtuel).
            if (w != width || h != height) handler.post { runCatching { startEncoder() }.onFailure { fail("Encodage impossible : ${it.message}") } }
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopAll(notifyPc = true, reason = "Arrêté sur le téléphone")
            return START_NOT_STICKY
        }
        val data = intent?.let { if (Build.VERSION.SDK_INT >= 33) it.getParcelableExtra(EXTRA_DATA, Intent::class.java) else @Suppress("DEPRECATION") it.getParcelableExtra(EXTRA_DATA) }
        if (data == null) { stopSelf(); return START_NOT_STICKY }
        // Une nouvelle demande remplace la diffusion en cours (nouvel accord = nouveau jeton).
        releaseAll()
        peer = intent.getStringExtra(EXTRA_PEER) ?: return START_NOT_STICKY
        val kind = intent.getStringExtra(EXTRA_KIND) ?: "mirror"
        val params = runCatching { JSONObject(intent.getStringExtra(EXTRA_PARAMS) ?: "{}") }.getOrDefault(JSONObject())
        video = kind == "mirror"
        audio = intent.getBooleanExtra(EXTRA_AUDIO_OK, false) && (kind == "phone_audio" || params.boolOrNull("with_audio") == true)
        maxSize = (params.intOrNull("max_size") ?: 1600).coerceIn(480, 2560)
        bitrate = (params.intOrNull("bitrate") ?: 8_000_000).coerceIn(1_000_000, 40_000_000)
        fps = (params.intOrNull("fps") ?: 60).coerceIn(15, 120)

        // Android 14+ : le service doit être au premier plan (type mediaProjection) avant d'obtenir la projection.
        ServiceCompat.startForeground(this, Notifs.ID_MIRROR, notification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION)
        val mp = runCatching { getSystemService(MediaProjectionManager::class.java).getMediaProjection(intent.getIntExtra(EXTRA_RESULT, 0), data) }.getOrNull()
        if (mp == null) { fail("Autorisation de diffusion invalide"); return START_NOT_STICKY }
        projection = mp
        mp.registerCallback(object : MediaProjection.Callback() {
            override fun onStop() {
                // Ignoré si cette projection a déjà été remplacée par une nouvelle demande.
                handler.post { if (projection === mp) stopAll(notifyPc = true, reason = "Diffusion arrêtée sur le téléphone") }
            }
        }, handler)
        running = true
        instance = this
        Core.node.setStreaming(true)
        handler.post {
            try {
                if (video) {
                    getSystemService(DisplayManager::class.java).registerDisplayListener(displayListener, handler)
                    startEncoder()
                }
                if (audio) startAudio()
                if (!video && !audio) fail("Autorisation d'enregistrer le son refusée")
            } catch (e: Exception) {
                Log.w(TAG, "démarrage", e)
                fail("Diffusion impossible : ${e.message}")
            }
        }
        return START_NOT_STICKY
    }

    private fun notification(): Notification {
        val stop = PendingIntent.getService(this, 1, Intent(this, MirrorService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE)
        return NotificationCompat.Builder(this, Notifs.CH_STREAM)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle(if (video) "Écran partagé avec le PC" else "Son partagé avec le PC")
            .setContentText("Touchez Arrêter pour couper la diffusion.")
            .setOngoing(true)
            .addAction(0, "Arrêter", stop)
            .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            .build()
    }

    /** Taille de l'écran réel ramenée à max_size (multiples de 16 pour les encodeurs). */
    private fun targetSize(limit: Int = maxSize): Triple<Int, Int, Int> {
        val wm = getSystemService(WindowManager::class.java)
        val (rw, rh) = if (Build.VERSION.SDK_INT >= 30) {
            wm.maximumWindowMetrics.bounds.let { it.width() to it.height() }
        } else {
            android.graphics.Point().also { @Suppress("DEPRECATION") wm.defaultDisplay.getRealSize(it) }.let { it.x to it.y }
        }
        val scale = minOf(1.0, limit.toDouble() / max(rw, rh))
        fun r16(v: Double) = max(16, ((v / 16).roundToInt()) * 16)
        return Triple(r16(rw * scale), r16(rh * scale), resources.displayMetrics.densityDpi)
    }

    private fun createCodec(w: Int, h: Int): Pair<MediaCodec, Surface> {
        val fmt = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, w, h).apply {
            setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface)
            setInteger(MediaFormat.KEY_BIT_RATE, bitrate)
            setInteger(MediaFormat.KEY_FRAME_RATE, fps)
            // Images clés à la demande du PC ; une toutes les 10 s par sécurité.
            setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 10)
            // Écran immobile : l'encodeur répète la dernière image pour garder le flux vivant.
            setLong(MediaFormat.KEY_REPEAT_PREVIOUS_FRAME_AFTER, 100_000L)
            setInteger(MediaFormat.KEY_PRIORITY, 0)
            setInteger(MediaFormat.KEY_MAX_B_FRAMES, 0)
            if (Build.VERSION.SDK_INT >= 30) setInteger(MediaFormat.KEY_LATENCY, 1)
        }
        val c = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_VIDEO_AVC)
        try {
            c.configure(fmt, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            val s = c.createInputSurface()
            c.start()
            return c to s
        } catch (e: Exception) {
            c.release()
            throw e
        }
    }

    private fun startEncoder() {
        var (w, h, dpi) = targetSize()
        // Certains encodeurs refusent les grandes définitions : on réessaie plus petit.
        var created: Pair<MediaCodec, Surface>? = null
        for (limit in listOf(maxSize, 1920, 1280, 960)) {
            if (limit > maxSize) continue
            val t = targetSize(limit)
            w = t.first; h = t.second; dpi = t.third
            created = runCatching { createCodec(w, h) }.getOrNull()
            if (created != null) break
        }
        val (c, s) = created ?: throw IllegalStateException("aucun encodeur H.264 disponible")
        val old = codec
        val oldSurface = inputSurface
        codec = c
        inputSurface = s
        width = w
        height = h
        val vd = display
        if (vd == null) {
            display = projection!!.createVirtualDisplay("nothing-connect", w, h, dpi, DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR, s, null, handler)
        } else {
            vd.resize(w, h, dpi)
            vd.surface = s
        }
        old?.let { o -> runCatching { o.stop() }; o.release() }
        oldSurface?.release()
        Core.send(peer, Streams.startMessage("mirror", JSONObject().put("codec", "h264").put("width", w).put("height", h).put("fps", fps)))
        Thread({ drain(c) }, "nc-mirror-out").start()
    }

    private fun drain(c: MediaCodec) {
        val info = MediaCodec.BufferInfo()
        var config: ByteArray? = null
        while (running && codec === c) {
            val idx = try { c.dequeueOutputBuffer(info, 100_000) } catch (_: IllegalStateException) { break }
            if (idx == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                // Certains encodeurs donnent SPS/PPS seulement dans le format de sortie.
                val f = c.outputFormat
                val sps = f.getByteBuffer("csd-0")
                val pps = f.getByteBuffer("csd-1")
                if (sps != null && config == null) {
                    val b = ByteArray(sps.remaining() + (pps?.remaining() ?: 0))
                    sps.get(b, 0, sps.remaining())
                    pps?.get(b, b.size - pps.remaining(), pps.remaining())
                    config = b
                    send(Flag.CONFIG, 0, b)
                }
                continue
            }
            if (idx < 0) continue
            try {
                val buf = c.getOutputBuffer(idx)
                if (buf != null && info.size > 0) {
                    val data = ByteArray(info.size)
                    buf.position(info.offset)
                    buf.get(data)
                    when {
                        info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0 -> {
                            config = data
                            send(Flag.CONFIG, info.presentationTimeUs, data)
                        }
                        info.flags and MediaCodec.BUFFER_FLAG_KEY_FRAME != 0 -> {
                            // Configuration renvoyée avant chaque image clé : un décodeur qui
                            // (re)démarre a toujours de quoi s'initialiser.
                            config?.let { send(Flag.CONFIG, info.presentationTimeUs, it) }
                            if (!send(Flag.KEYFRAME, info.presentationTimeUs, data)) requestKeyframe()
                        }
                        // Image perdue (réseau saturé) : la suivante ne serait pas décodable.
                        else -> if (!send(0, info.presentationTimeUs, data)) requestKeyframe()
                    }
                }
                c.releaseOutputBuffer(idx, false)
            } catch (_: IllegalStateException) {
                break
            }
        }
    }

    private fun send(flags: Int, ts: Long, data: ByteArray): Boolean =
        runCatching { Core.node.sendStream(peer, Kind.MIRROR.toUByte(), flags.toUByte(), ts.toULong(), data) }.getOrDefault(false)

    fun requestKeyframe() {
        val now = SystemClock.elapsedRealtime()
        if (now - lastKeyRequest < 300) return
        lastKeyRequest = now
        runCatching { codec?.setParameters(Bundle().apply { putInt(MediaCodec.PARAMETER_KEY_REQUEST_SYNC_FRAME, 0) }) }
    }

    @SuppressLint("MissingPermission")
    private fun startAudio() {
        val cfg = AudioPlaybackCaptureConfiguration.Builder(projection!!)
            .addMatchingUsage(AudioAttributes.USAGE_MEDIA)
            .addMatchingUsage(AudioAttributes.USAGE_GAME)
            .addMatchingUsage(AudioAttributes.USAGE_UNKNOWN)
            .build()
        val format = AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_16BIT).setSampleRate(RATE).setChannelMask(AudioFormat.CHANNEL_IN_STEREO).build()
        val min = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_STEREO, AudioFormat.ENCODING_PCM_16BIT)
        val rec = AudioRecord.Builder().setAudioFormat(format).setBufferSizeInBytes(max(min, CHUNK * 8)).setAudioPlaybackCaptureConfig(cfg).build()
        if (rec.state != AudioRecord.STATE_INITIALIZED) {
            rec.release()
            Core.send(peer, Streams.stopMessage("phone_audio", "Capture du son indisponible sur ce téléphone"))
            return
        }
        record = rec
        rec.startRecording()
        Core.send(peer, Streams.startMessage("phone_audio", JSONObject().put("codec", "pcm_s16le").put("sample_rate", RATE).put("channels", 2)))
        Thread({
            val t0 = SystemClock.elapsedRealtimeNanos()
            while (running && record === rec) {
                val buf = ByteArray(CHUNK)
                var n = 0
                while (n < CHUNK) {
                    val r = rec.read(buf, n, CHUNK - n)
                    if (r <= 0) break
                    n += r
                }
                if (n < CHUNK) { if (!running) break else continue }
                val ts = (SystemClock.elapsedRealtimeNanos() - t0) / 1000
                runCatching { Core.node.sendStream(peer, Kind.PHONE_AUDIO.toUByte(), 0u, ts.toULong(), buf) }
            }
        }, "nc-phone-audio").start()
    }

    fun stopIfPeer(p: String) {
        if (running && ::peer.isInitialized && peer == p) handler.post { stopAll(notifyPc = false) }
    }

    private fun fail(reason: String) {
        stopAll(notifyPc = true, reason = reason)
    }

    private fun releaseAll() {
        running = false
        runCatching { getSystemService(DisplayManager::class.java).unregisterDisplayListener(displayListener) }
        codec?.let { c -> codec = null; runCatching { c.stop() }; runCatching { c.release() } }
        inputSurface?.release(); inputSurface = null
        display?.release(); display = null
        record?.let { r -> record = null; runCatching { r.stop() }; r.release() }
        projection?.let { p -> projection = null; runCatching { p.stop() } }
    }

    fun stopAll(notifyPc: Boolean, reason: String? = null) {
        val wasRunning = running
        val hadVideo = video
        val hadAudio = audio
        releaseAll()
        if (notifyPc && wasRunning && ::peer.isInitialized) {
            if (hadVideo) Core.send(peer, Streams.stopMessage("mirror", reason))
            if (hadAudio || !hadVideo) Core.send(peer, Streams.stopMessage("phone_audio", reason))
        }
        if (instance === this) instance = null
        if (wasRunning) runCatching { Core.node.setStreaming(false) }
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        releaseAll()
        if (instance === this) instance = null
        thread.quitSafely()
        super.onDestroy()
    }

    companion object {
        private const val TAG = "NcMirror"
        const val RATE = 48_000
        /** 10 ms de PCM 16 bits stéréo. */
        const val CHUNK = RATE / 100 * 4
        const val ACTION_STOP = "stop"
        const val EXTRA_DATA = "data"
        const val EXTRA_RESULT = "result"
        const val EXTRA_PEER = "peer"
        const val EXTRA_KIND = "kind"
        const val EXTRA_PARAMS = "params"
        const val EXTRA_AUDIO_OK = "audio_ok"

        @Volatile
        var instance: MirrorService? = null

        fun start(ctx: Context, result: Int, data: Intent, peer: String, kind: String, params: JSONObject, audioOk: Boolean) {
            val i = Intent(ctx, MirrorService::class.java)
                .putExtra(EXTRA_RESULT, result).putExtra(EXTRA_DATA, data).putExtra(EXTRA_PEER, peer)
                .putExtra(EXTRA_KIND, kind).putExtra(EXTRA_PARAMS, params.toString()).putExtra(EXTRA_AUDIO_OK, audioOk)
            ctx.startForegroundService(i)
        }

        /** Le PC arrête le miroir ou le son : on coupe ce qui a été demandé. */
        fun stopFromPc(ctx: Context, kind: String) {
            val s = instance ?: return
            s.handler.post {
                if (kind == "mirror" || !s.video) s.stopAll(notifyPc = false)
                else {
                    s.record?.let { r -> s.record = null; runCatching { r.stop() }; r.release() }
                    s.audio = false
                }
            }
        }
    }
}
