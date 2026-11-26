package io.github.nabeiroriviereadriano.nothingconnect.stream

import android.content.Context
import android.content.Intent
import android.content.pm.ActivityInfo
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Rect
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.MotionEvent
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import org.json.JSONObject
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Le téléphone devient un écran supplémentaire du PC : plein écran, images JPEG du moniteur
 * virtuel Windows, et toucher renvoyé au PC (souris).
 */
class DisplayActivity : ComponentActivity(), SurfaceHolder.Callback {
    private lateinit var peer: String
    private lateinit var view: SurfaceView
    private var surfaceOk = false
    private var announced = false
    private var stoppedByPc = false
    private val main = Handler(Looper.getMainLooper())
    private val decoder = Executors.newSingleThreadExecutor()
    private val pending = AtomicReference<ByteArray?>(null)
    private val decoding = AtomicBoolean(false)
    private var reuse: Bitmap? = null
    private val paint = Paint(Paint.FILTER_BITMAP_FLAG)
    private val dst = Rect()

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
        WindowCompat.setDecorFitsSystemWindows(window, false)
        view = SurfaceView(this)
        view.holder.addCallback(this)
        view.setOnTouchListener { _, e -> onTouch(e); true }
        setContentView(view)
        WindowInsetsControllerCompat(window, view).apply {
            hide(WindowInsetsCompat.Type.systemBars())
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
        // Une seule session d'écran étendu à la fois.
        instance?.takeIf { it !== this }?.let { it.stoppedByPc = true; it.finish() }
        instance = this
    }

    override fun surfaceCreated(holder: SurfaceHolder) {}

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        surfaceOk = true
        // La taille définitive arrive après la rotation et le masquage des barres : on attend
        // qu'elle se stabilise avant de l'annoncer au PC (qui crée un moniteur de cette taille).
        main.removeCallbacksAndMessages(ANNOUNCE)
        main.postAtTime({ announce(width, height) }, ANNOUNCE, android.os.SystemClock.uptimeMillis() + 450)
        if (announced) requestFrame()
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        surfaceOk = false
    }

    private fun announce(w: Int, h: Int) {
        if (announced || isFinishing) return
        announced = true
        val hz = (if (Build.VERSION.SDK_INT >= 30) display?.refreshRate else @Suppress("DEPRECATION") windowManager.defaultDisplay.refreshRate)?.toInt() ?: 60
        val params = JSONObject().put("codec", "jpeg").put("width", w).put("height", h)
            .put("dpi", resources.displayMetrics.densityDpi).put("refresh_hz", hz.coerceIn(30, 120))
        Core.send(peer, Streams.startMessage("display", params))
    }

    private fun requestFrame() {
        Core.send(peer, JSONObject().put("t", "stream_keyframe").put("kind", "display"))
    }

    /** Appelé sur le fil réseau : on garde seulement la dernière image (pas de retard accumulé). */
    private fun frame(data: ByteArray) {
        pending.set(data)
        if (decoding.compareAndSet(false, true)) decoder.execute { drawLoop() }
    }

    private fun drawLoop() {
        try {
            while (true) {
                val data = pending.getAndSet(null) ?: break
                if (!surfaceOk) continue
                val opts = BitmapFactory.Options().apply {
                    inMutable = true
                    inPreferredConfig = Bitmap.Config.ARGB_8888
                    inBitmap = reuse
                }
                val bmp = runCatching { BitmapFactory.decodeByteArray(data, 0, data.size, opts) }.getOrNull()
                    ?: runCatching { BitmapFactory.decodeByteArray(data, 0, data.size, BitmapFactory.Options().apply { inMutable = true }) }.getOrNull()
                    ?: continue
                reuse = bmp
                val holder = view.holder
                val canvas = runCatching { if (Build.VERSION.SDK_INT >= 26) holder.lockHardwareCanvas() else holder.lockCanvas() }.getOrNull() ?: continue
                try {
                    // Même rapport largeur/hauteur que le moniteur : l'image remplit l'écran.
                    val sw = canvas.width.toFloat() / bmp.width
                    val sh = canvas.height.toFloat() / bmp.height
                    val s = minOf(sw, sh)
                    val w = (bmp.width * s).toInt()
                    val h = (bmp.height * s).toInt()
                    dst.set((canvas.width - w) / 2, (canvas.height - h) / 2, (canvas.width + w) / 2, (canvas.height + h) / 2)
                    canvas.drawColor(Color.BLACK)
                    canvas.drawBitmap(bmp, null, dst, paint)
                } finally {
                    holder.unlockCanvasAndPost(canvas)
                }
            }
        } finally {
            decoding.set(false)
            if (pending.get() != null && decoding.compareAndSet(false, true)) decoder.execute { drawLoop() }
        }
    }

    private fun onTouch(e: MotionEvent) {
        val w = view.width.toFloat().coerceAtLeast(1f)
        val h = view.height.toFloat().coerceAtLeast(1f)
        fun send(action: String, i: Int) {
            Core.send(peer, JSONObject().put("t", "display_pointer").put("action", action).put("pointer", e.getPointerId(i))
                .put("x", (e.getX(i) / w).coerceIn(0f, 1f).toDouble()).put("y", (e.getY(i) / h).coerceIn(0f, 1f).toDouble()))
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
        decoder.shutdownNow()
        super.onDestroy()
    }

    companion object {
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

        fun onFrame(peer: String, data: ByteArray) {
            instance?.takeIf { it.peer == peer }?.frame(data)
        }

        fun stopFromPc() {
            instance?.let { a -> a.main.post { a.stoppedByPc = true; a.finish() } }
        }

        fun stopIfPeer(peer: String) {
            instance?.takeIf { it.peer == peer }?.let { a -> a.main.post { a.stoppedByPc = true; a.finish() } }
        }
    }
}
