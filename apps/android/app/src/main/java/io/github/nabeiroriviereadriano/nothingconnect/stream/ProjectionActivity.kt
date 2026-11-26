package io.github.nabeiroriviereadriano.nothingconnect.stream

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.projection.MediaProjectionConfig
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import org.json.JSONObject

/**
 * Demande l'autorisation Android de diffuser l'écran et/ou le son (fenêtre système),
 * puis démarre le service de diffusion. Aucune diffusion sans accord explicite.
 */
class ProjectionActivity : ComponentActivity() {
    private lateinit var peer: String
    private lateinit var kind: String
    private lateinit var params: JSONObject

    private val consent = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        if (r.resultCode == Activity.RESULT_OK && r.data != null) {
            MirrorService.start(this, r.resultCode, r.data!!, peer, kind, params, audioAllowed())
        } else {
            Core.send(peer, Streams.stopMessage(kind, "Refusé sur le téléphone"))
        }
        finish()
    }

    private val micPerm = registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (!granted && kind == "phone_audio") {
            Core.send(peer, Streams.stopMessage(kind, "Autorisation d'enregistrer le son refusée sur le téléphone"))
            finish()
        } else {
            askProjection()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Streams.clearRequestNotification(this)
        peer = intent.getStringExtra(EXTRA_PEER) ?: return finish()
        kind = intent.getStringExtra(EXTRA_KIND) ?: return finish()
        params = runCatching { JSONObject(intent.getStringExtra(EXTRA_PARAMS) ?: "{}") }.getOrDefault(JSONObject())
        if (savedInstanceState != null) return
        val wantsAudio = kind == "phone_audio" || params.boolOrNull("with_audio") == true
        if (wantsAudio && !audioAllowed()) micPerm.launch(Manifest.permission.RECORD_AUDIO) else askProjection()
    }

    private fun audioAllowed() = checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    private fun askProjection() {
        val mpm = getSystemService(MediaProjectionManager::class.java)
        // Android 14+ : écran entier (le PC affiche tout le téléphone, pas une seule appli).
        val intent = if (Build.VERSION.SDK_INT >= 34) mpm.createScreenCaptureIntent(MediaProjectionConfig.createConfigForDefaultDisplay())
        else mpm.createScreenCaptureIntent()
        consent.launch(intent)
    }

    companion object {
        const val EXTRA_PEER = "peer"
        const val EXTRA_KIND = "kind"
        const val EXTRA_PARAMS = "params"

        fun request(ctx: Context, peer: String, kind: String, params: JSONObject) {
            val i = Intent(ctx, ProjectionActivity::class.java)
                .putExtra(EXTRA_PEER, peer).putExtra(EXTRA_KIND, kind).putExtra(EXTRA_PARAMS, params.toString())
            val name = Core.devices.value.firstOrNull { it.id == peer }?.name ?: "Le PC"
            val what = if (kind == "mirror") "afficher l'écran du téléphone" else "diffuser le son du téléphone"
            Streams.openOrNotify(ctx, i, "$name veut $what", "Touchez pour autoriser ou refuser.")
        }
    }
}
