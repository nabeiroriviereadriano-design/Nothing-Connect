package io.github.nabeiroriviereadriano.nothingconnect.core

import android.content.Context
import android.os.Build
import android.provider.Settings
import io.github.nabeiroriviereadriano.nothingconnect.BuildConfig
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject
import uniffi.nc_ffi.EventListener
import uniffi.nc_ffi.NamedInput
import uniffi.nc_ffi.NcException
import uniffi.nc_ffi.NcNode
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList

data class PcDevice(val id: String, val name: String, val appVersion: String?, val lastSeen: Long?, val autoAccept: Boolean)

data class TransferUi(
    val id: String,
    val outgoing: Boolean,
    val state: String,
    val done: Long,
    val total: Long,
    val speedBps: Long,
    val etaS: Long?,
    val file: String?,
    val error: String? = null,
)

data class IncomingUi(val id: String, val peerId: String, val files: List<String>, val total: Long)

sealed interface PairingUi {
    data object Idle : PairingUi
    data object Waiting : PairingUi
    data class Failed(val reason: String) : PairingUi
    data class Done(val name: String) : PairingUi
}

/**
 * Point d'entrée unique vers le cœur Rust. Les événements arrivent sur des threads
 * natifs : ils sont recopiés immédiatement dans une file Kotlin traitée en série.
 */
@OptIn(ExperimentalCoroutinesApi::class)
object Core : EventListener {
    lateinit var app: Context
        private set
    lateinit var node: NcNode
        private set
    lateinit var prefs: Prefs
        private set

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default.limitedParallelism(1))
    private var started = false

    val connected = MutableStateFlow<Map<String, String>>(emptyMap())
    val devices = MutableStateFlow<List<PcDevice>>(emptyList())
    val transfers = MutableStateFlow<Map<String, TransferUi>>(emptyMap())
    val incoming = MutableStateFlow<List<IncomingUi>>(emptyList())
    val pairing = MutableStateFlow<PairingUi>(PairingUi.Idle)
    val refused = MutableStateFlow<Set<String>>(emptySet())
    val messages = MutableSharedFlow<String>(extraBufferCapacity = 16)

    /** Abonnés internes (service, notifications, multimédia…). */
    private val listeners = CopyOnWriteArrayList<(JSONObject) -> Unit>()
    fun subscribe(l: (JSONObject) -> Unit) { listeners += l }
    fun unsubscribe(l: (JSONObject) -> Unit) { listeners -= l }

    val receivedDir: File get() = File(app.getExternalFilesDir(null) ?: app.filesDir, "received")

    fun capabilities(): JSONArray = JSONArray(
        listOf(
            "file_transfer", "share_text", "device_status", "notifications", "notification_actions",
            "media_control", "ring", "vibrate", "media_volume", "do_not_disturb",
        )
    )

    fun deviceName(ctx: Context): String =
        Settings.Global.getString(ctx.contentResolver, Settings.Global.DEVICE_NAME)?.takeIf { it.isNotBlank() } ?: Build.MODEL

    fun init(ctx: Context) {
        app = ctx.applicationContext
        prefs = Prefs(app)
        val device = JSONObject()
            .put("name", deviceName(app))
            .put("platform", "android")
            .put("manufacturer", Build.MANUFACTURER.replaceFirstChar { it.uppercase() })
            .put("model", Build.MODEL)
            .put("os_version", Build.VERSION.RELEASE)
            .put("app_version", BuildConfig.VERSION_NAME)
            .put("capabilities", capabilities())
        receivedDir.mkdirs()
        node = NcNode(
            File(app.filesDir, "core").absolutePath,
            receivedDir.absolutePath,
            device.toString(),
            KeystoreVault(app),
            this,
        )
        refreshDevices()
    }

    @Synchronized
    fun start() {
        if (started) return
        try {
            node.start()
            started = true
        } catch (e: NcException) {
            messages.tryEmit(errorText(e))
        }
    }

    fun errorText(e: Throwable): String = (e as? NcException.Failed)?.msg ?: e.message ?: "Erreur inconnue"

    /** Exécute un appel au cœur et signale l'erreur éventuelle à l'interface. */
    inline fun <T> call(block: () -> T): T? = try {
        block()
    } catch (e: Exception) {
        messages.tryEmit(errorText(e))
        null
    }

    fun refreshDevices() {
        val arr = try { JSONArray(node.devicesJson()) } catch (e: Exception) { JSONArray() }
        devices.value = (0 until arr.length()).map { arr.getJSONObject(it) }
            .filter { it.optString("platform") == "windows" }
            .map {
                PcDevice(
                    id = it.getString("id"),
                    name = it.optString("custom_name").takeIf { n -> n.isNotBlank() && n != "null" } ?: it.getString("name"),
                    appVersion = it.optString("app_version").takeIf { v -> v.isNotBlank() && v != "null" },
                    lastSeen = if (it.isNull("last_seen")) null else it.getLong("last_seen"),
                    autoAccept = it.optBoolean("auto_accept"),
                )
            }
    }

    fun broadcast(msg: JSONObject): Int = try { node.broadcastJson(msg.toString()).toInt() } catch (e: Exception) { 0 }

    fun send(peerId: String, msg: JSONObject) {
        try { node.sendJson(peerId, msg.toString()) } catch (_: Exception) { }
    }

    fun sendFiles(peerId: String, inputs: List<NamedInput>): String? = call { node.sendFiles(peerId, inputs) }

    override fun onEvent(json: String) {
        scope.launch {
            val e = try { JSONObject(json) } catch (_: Exception) { return@launch }
            handle(e)
            listeners.forEach { l -> runCatching { l(e) } }
        }
    }

    private fun handle(e: JSONObject) {
        when (e.getString("type")) {
            "peer_connected" -> {
                connected.update { it + (e.getString("peer_id") to e.getString("addr")) }
                refused.update { it - e.getString("peer_id") }
                refreshDevices()
            }
            "peer_disconnected" -> connected.update { it - e.getString("peer_id") }
            "peer_refused" -> refused.update { it + e.getString("peer_id") }
            "paired" -> {
                pairing.value = PairingUi.Done(e.getJSONObject("device").getString("name"))
                refreshDevices()
            }
            "pairing_failed" -> pairing.value = PairingUi.Failed(e.getString("reason"))
            "device_removed" -> {
                connected.update { it - e.getString("peer_id") }
                refreshDevices()
            }
            "incompatible_version" -> messages.tryEmit(
                if (e.optInt("remote") > e.optInt("local")) "Le PC utilise une version plus récente : mettez à jour l'application."
                else "Le PC utilise une ancienne version : mettez à jour NOTHING CONNECT sur Windows."
            )
            "transfer_incoming" -> {
                val files = e.getJSONArray("files")
                val names = (0 until files.length()).map { files.getJSONObject(it).getString("rel_path") }
                incoming.update { l -> l.filter { it.id != e.getString("id") } + IncomingUi(e.getString("id"), e.getString("peer_id"), names, e.getLong("total_bytes")) }
            }
            "transfer_progress" -> {
                val id = e.getString("id")
                transfers.update {
                    it + (id to TransferUi(
                        id = id,
                        outgoing = e.getBoolean("outgoing"),
                        state = e.getString("state"),
                        done = e.getLong("bytes_done"),
                        total = e.getLong("total_bytes"),
                        speedBps = e.getLong("speed_bps"),
                        etaS = if (e.isNull("eta_s")) null else e.getLong("eta_s"),
                        file = e.optString("current_file").takeIf { f -> f.isNotBlank() && f != "null" },
                    ))
                }
                if (e.getString("state") != "waiting") incoming.update { l -> l.filter { it.id != id } }
            }
            "transfer_finished" -> {
                val id = e.getString("id")
                transfers.update { m ->
                    val t = m[id] ?: return@update m
                    m + (id to t.copy(state = e.getString("state"), error = e.optString("error").takeIf { s -> s.isNotBlank() && s != "null" }))
                }
                incoming.update { l -> l.filter { it.id != id } }
            }
            "error" -> messages.tryEmit(e.getString("message"))
        }
    }
}
