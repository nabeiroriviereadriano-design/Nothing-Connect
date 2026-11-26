package io.github.nabeiroriviereadriano.nothingconnect.stream

import android.content.Context
import android.content.Intent
import android.content.pm.ActivityInfo
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Rect
import android.graphics.RectF
import android.media.MediaCodec
import android.media.MediaFormat
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import android.view.Display
import android.view.Gravity
import android.view.MotionEvent
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.WindowManager
import android.widget.FrameLayout
import androidx.activity.ComponentActivity
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import org.json.JSONObject
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Le téléphone devient un écran supplémentaire du PC : plein écran, à la fréquence maximale de
 * l'écran. Le PC envoie de la vidéo H.264 (décodée par le circuit vidéo du téléphone) ou, pour
 * les anciennes versions, des images JPEG. Le toucher est renvoyé au PC (souris).
 */
class DisplayActivity : ComponentActivity(), SurfaceHolder.Callback {
    private lateinit var peer: String
    private lateinit var root: FrameLayout
    private lateinit var view: SurfaceView
    @Volatile private var surfaceOk = false
    private var announced = false
    private var stoppedByPc = false
    private val main = Handler(Looper.getMainLooper())
    private var maxHz = 60

    /** Format choisi par le PC dans sa réponse : « h264 » ou « jpeg ». */
    @Volatile private var codec = "jpeg"
    private var pcW = 0
    private var pcH = 0

    // --- H.264
    private class Packet(val flags: Int, val ts: Long, val data: ByteArray)
    private val packets = LinkedBlockingQueue<Packet>(90)
    @Volatile private var decoder: MediaCodec? = null
    private var decodeThread: Thread? = null
    @Volatile private var waitingKey = true
    private var lastKeyAsk = 0L

    // --- JPEG (ancien PC)
    private val jpegExec = Executors.newSingleThreadExecutor()
    private val pending = AtomicReference<ByteArray?>(null)
    private val decoding = AtomicBoolean(false)
    private var reuse: Bitmap? = null
    private val paint = Paint(Paint.FILTER_BITMAP_FLAG)
    private val dst = Rect()
    @Volatile private var image = RectF()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Streams.clearRequestNotification(this)
        peer = intent.getStringExtra(EXTRA_PEER) ?: return finish()
        val params = runCatching { JSONObject(intent.getStringExtra(EXTRA_PARAMS) ?: "{}") }.getOrDefault(JSONObject())
        requestedOrientation = if (params.boolOrNull("portrait") == true) ActivityInfo.SCREEN_ORIENTATION_SENSOR_PORTRAIT
        else ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        if (Build.VERSION.SDK_INT >= 30) window.attributes.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
        else @Suppress("DEPRECATION") { window.attributes.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES }
        preferMaxRefreshRate()
        WindowCompat.setDecorFitsSystemWindows(window, false)
        root = FrameLayout(this).apply { setBackgroundColor(Color.BLACK) }
        view = SurfaceView(this)
        view.holder.addCallback(this)
        root.addView(view, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT, Gravity.CENTER))
        root.setOnTouchListener { _, e -> onTouch(e); true }
        setContentView(root)
        WindowInsetsControllerCompat(window, root).apply {
            hide(WindowInsetsCompat.Type.systemBars())
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
        // Une seule session d'écran étendu à la fois.
        instance?.takeIf { it !== this }?.let { it.stoppedByPc = true; it.finish() }
        instance = this
    }

    private fun currentDisplay(): Display? =
        if (Build.VERSION.SDK_INT >= 30) display else @Suppress("DEPRECATION") windowManager.defaultDisplay

    /** Demande à Android le mode d'affichage le plus rapide à la définition actuelle (90, 120 Hz…). */
    private fun preferMaxRefreshRate() {
        val d = currentDisplay() ?: return
        val cur = d.mode
        val best = d.supportedModes
            .filter { it.physicalWidth == cur.physicalWidth && it.physicalHeight == cur.physicalHeight }
            .maxByOrNull { it.refreshRate } ?: cur
        maxHz = best.refreshRate.toInt().coerceIn(30, 240)
        window.attributes = window.attributes.also { it.preferredDisplayModeId = best.modeId }
    }

    override fun surfaceCreated(holder: SurfaceHolder) {
        if (Build.VERSION.SDK_INT >= 30) {
            runCatching { holder.surface.setFrameRate(maxHz.toFloat(), Surface.FRAME_RATE_COMPATIBILITY_DEFAULT) }
        }
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        surfaceOk = true
        if (!announced) {
            // La taille définitive arrive après la rotation et le masquage des barres : on attend
            // qu'elle se stabilise avant de l'annoncer au PC (qui crée un moniteur de cette taille).
            main.removeCallbacksAndMessages(ANNOUNCE)
            main.postAtTime({ announce(width, height) }, ANNOUNCE, SystemClock.uptimeMillis() + 450)
        } else {
            if (codec == "h264" && decoder == null) startDecoder()
            askKeyframe(force = true)
        }
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        surfaceOk = false
        stopDecoder()
    }

    private fun announce(w: Int, h: Int) {
        if (announced || isFinishing) return
        announced = true
        val params = JSONObject().put("codec", "h264").put("width", w).put("height", h)
            .put("dpi", resources.displayMetrics.densityDpi).put("refresh_hz", maxHz)
        Core.send(peer, Streams.startMessage("display", params))
    }

    private fun askKeyframe(force: Boolean = false) {
        val now = SystemClock.elapsedRealtime()
        if (!force && now - lastKeyAsk < 250) return
        lastKeyAsk = now
        Core.send(peer, JSONObject().put("t", "stream_keyframe").put("kind", "display"))
    }

    /** Réponse du PC : format et définition réels du flux. */
    private fun onPcStart(params: JSONObject) {
        val c = params.optString("codec", "jpeg")
        val w = params.intOrNull("width") ?: 0
        val h = params.intOrNull("height") ?: 0
        val changed = c != codec || w != pcW || h != pcH
        codec = c
        pcW = w
        pcH = h
        fitSurface()
        if (c == "h264") {
            if (changed || decoder == null) {
                stopDecoder()
                if (surfaceOk) startDecoder()
            }
        } else {
            stopDecoder()
        }
    }

    /** La zone d'affichage prend exactement le format de l'image du PC (bandes noires si besoin). */
    private fun fitSurface() {
        if (pcW <= 0 || pcH <= 0 || root.width == 0) return
        val s = minOf(root.width.toFloat() / pcW, root.height.toFloat() / pcH)
        val w = (pcW * s).toInt()
        val h = (pcH * s).toInt()
        val lp = view.layoutParams as FrameLayout.LayoutParams
        if (lp.width != w || lp.height != h) {
            lp.width = w
            lp.height = h
            lp.gravity = Gravity.CENTER
            view.layoutParams = lp
        }
    }

    // ------------------------------------------------------------------ H.264

    private fun startDecoder() {
        if (decoder != null || pcW <= 0 || !surfaceOk) return
        val c = try {
            val fmt = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, pcW, pcH).apply {
                setInteger(MediaFormat.KEY_PRIORITY, 0)
                setInteger(MediaFormat.KEY_OPERATING_RATE, maxHz.coerceAtLeast(60))
                if (Build.VERSION.SDK_INT >= 30) setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
                // Clés de faible latence propres à certains fabricants (ignorées ailleurs).
                setInteger("vendor.qti-ext-dec-low-latency.enable", 1)
                setInteger("vendor.low-latency.enable", 1)
            }
            MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_VIDEO_AVC).also {
                it.configure(fmt, view.holder.surface, null, 0)
                it.setVideoScalingMode(MediaCodec.VIDEO_SCALING_MODE_SCALE_TO_FIT)
                it.start()
            }
        } catch (e: Exception) {
            Log.w(TAG, "décodeur H.264", e)
            return
        }
        decoder = c
        waitingKey = true
        packets.clear()
        decodeThread = Thread({ decodeLoop(c) }, "nc-display-decode").apply { start() }
        askKeyframe(force = true)
    }

    private fun stopDecoder() {
        val c = decoder ?: return
        decoder = null
        decodeThread?.join(300)
        decodeThread = null
        runCatching { c.stop() }
        runCatching { c.release() }
    }

    private fun decodeLoop(c: MediaCodec) {
        val info = MediaCodec.BufferInfo()
        var config: ByteArray? = null
        while (decoder === c) {
            val p = packets.poll(4, TimeUnit.MILLISECONDS)
            try {
                if (p != null) {
                    when {
                        // SPS/PPS : envoyés avec l'image clé qui suit (forme acceptée par tous les décodeurs).
                        p.flags and Flag.CONFIG != 0 -> config = p.data
                        waitingKey && p.flags and Flag.KEYFRAME == 0 -> askKeyframe()
                        else -> {
                            val key = p.flags and Flag.KEYFRAME != 0
                            val data = if (key && config != null) config!! + p.data else p.data
                            val idx = c.dequeueInputBuffer(20_000)
                            if (idx >= 0) {
                                val buf = c.getInputBuffer(idx)!!
                                buf.clear()
                                buf.put(data)
                                c.queueInputBuffer(idx, 0, data.size, p.ts, 0)
                                if (key) waitingKey = false
                            } else {
                                // Décodeur saturé : on repart de la prochaine image clé.
                                waitingKey = true
                                packets.clear()
                                askKeyframe()
                            }
                        }
                    }
                }
                // Affichage immédiat de chaque image décodée.
                while (true) {
                    val o = c.dequeueOutputBuffer(info, 0)
                    if (o >= 0) c.releaseOutputBuffer(o, true)
                    else if (o != MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) break
                }
            } catch (e: Exception) {
                if (decoder !== c) break
                Log.w(TAG, "décodage", e)
                main.post { if (decoder === c) { stopDecoder(); startDecoder() } }
                break
            }
        }
    }

    private fun onPacket(flags: Int, ts: Long, data: ByteArray) {
        if (codec != "h264") { jpegFrame(data); return }
        if (decoder == null) return
        if (!packets.offer(Packet(flags, ts, data))) {
            // Retard : on vide la file et on attend une image clé plutôt que d'afficher en retard.
            packets.clear()
            waitingKey = true
            askKeyframe()
        }
    }

    // ------------------------------------------------------------------ JPEG (ancien PC)

    private fun jpegFrame(data: ByteArray) {
        pending.set(data)
        if (decoding.compareAndSet(false, true)) jpegExec.execute { drawLoop() }
    }

    private fun drawLoop() {
        try {
            while (true) {
                val data = pending.getAndSet(null) ?: break
                if (!surfaceOk) continue
                val opts = BitmapFactory.Options().apply { inMutable = true; inBitmap = reuse }
                val bmp = runCatching { BitmapFactory.decodeByteArray(data, 0, data.size, opts) }.getOrNull()
                    ?: runCatching { BitmapFactory.decodeByteArray(data, 0, data.size, BitmapFactory.Options().apply { inMutable = true }) }.getOrNull()
                    ?: continue
                reuse = bmp
                val holder = view.holder
                val canvas = runCatching { holder.lockHardwareCanvas() }.getOrNull() ?: continue
                try {
                    val s = minOf(canvas.width.toFloat() / bmp.width, canvas.height.toFloat() / bmp.height)
                    val w = (bmp.width * s).toInt()
                    val h = (bmp.height * s).toInt()
                    dst.set((canvas.width - w) / 2, (canvas.height - h) / 2, (canvas.width + w) / 2, (canvas.height + h) / 2)
                    canvas.drawColor(Color.BLACK)
                    canvas.drawBitmap(bmp, null, dst, paint)
                    image = RectF(dst)
                } finally {
                    holder.unlockCanvasAndPost(canvas)
                }
            }
        } finally {
            decoding.set(false)
            if (pending.get() != null && decoding.compareAndSet(false, true)) jpegExec.execute { drawLoop() }
        }
    }

    // ------------------------------------------------------------------ toucher

    private fun onTouch(e: MotionEvent) {
        // Coordonnées rapportées à l'image affichée (jamais aux bandes noires).
        val vx = view.x
        val vy = view.y
        val r = if (codec == "jpeg" && image.width() > 1f) RectF(image).apply { offset(vx, vy) }
        else RectF(vx, vy, vx + view.width, vy + view.height)
        fun send(action: String, i: Int) {
            val x = ((e.getX(i) - r.left) / r.width().coerceAtLeast(1f)).coerceIn(0f, 1f)
            val y = ((e.getY(i) - r.top) / r.height().coerceAtLeast(1f)).coerceIn(0f, 1f)
            Core.send(peer, JSONObject().put("t", "display_pointer").put("action", action).put("pointer", e.getPointerId(i))
                .put("x", x.toDouble()).put("y", y.toDouble()))
        }
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> send("down", e.actionIndex)
            MotionEvent.ACTION_MOVE -> for (i in 0 until e.pointerCount) send("move", i)
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> send("up", e.actionIndex)
            MotionEvent.ACTION_CANCEL -> send("cancel", 0)
        }
    }

    override fun onDestroy() {
        main.removeCallbacksAndMessages(null)
        if (instance === this) instance = null
        if (!stoppedByPc && announced) Core.send(peer, Streams.stopMessage("display"))
        stopDecoder()
        jpegExec.shutdownNow()
        super.onDestroy()
    }

    companion object {
        private const val TAG = "NcDisplay"
        private const val EXTRA_PEER = "peer"
        private const val EXTRA_PARAMS = "params"
        private val ANNOUNCE = Any()

        @Volatile
        private var instance: DisplayActivity? = null

        fun request(ctx: Context, peer: String, params: JSONObject) {
            val i = Intent(ctx, DisplayActivity::class.java).putExtra(EXTRA_PEER, peer).putExtra(EXTRA_PARAMS, params.toString())
            val name = Core.devices.value.firstOrNull { it.id == peer }?.name ?: "Le PC"
            Streams.openOrNotify(ctx, i, "$name veut utiliser ce téléphone comme écran", "Touchez pour afficher l'écran étendu.")
        }

        /** Appelé sur le fil réseau. */
        fun onFrame(peer: String, flags: Int, ts: Long, data: ByteArray) {
            instance?.takeIf { it.peer == peer }?.onPacket(flags, ts, data)
        }

        fun onPcStart(peer: String, params: JSONObject) {
            instance?.takeIf { it.peer == peer }?.let { a -> a.main.post { a.onPcStart(params) } }
        }

        fun stopFromPc() {
            instance?.let { a -> a.main.post { a.stoppedByPc = true; a.finish() } }
        }

        fun stopIfPeer(peer: String) {
            instance?.takeIf { it.peer == peer }?.let { a -> a.main.post { a.stoppedByPc = true; a.finish() } }
        }

        /** Fréquence maximale de l'écran du téléphone (annoncée au PC dans l'état de l'appareil). */
        fun maxRefreshRate(ctx: Context): Int {
            val d = ctx.getSystemService(android.hardware.display.DisplayManager::class.java).getDisplay(Display.DEFAULT_DISPLAY) ?: return 60
            val cur = d.mode
            return d.supportedModes.filter { it.physicalWidth == cur.physicalWidth && it.physicalHeight == cur.physicalHeight }
                .maxOfOrNull { it.refreshRate.toInt() } ?: cur.refreshRate.toInt()
        }
    }
}
