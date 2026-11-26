package io.github.nabeiroriviereadriano.nothingconnect.core

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import uniffi.nc_ffi.SecretVault
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Coffre de secrets : chaque secret est chiffré en AES-256-GCM avec une clé non
 * exportable de l'Android Keystore, puis stocké chiffré dans des préférences privées.
 * La clé privée d'identité n'existe donc jamais en clair sur le stockage.
 */
class KeystoreVault(context: Context) : SecretVault {
    private val prefs = context.getSharedPreferences("nc_vault", Context.MODE_PRIVATE)
    private val alias = "nothing-connect-vault"

    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getEntry(alias, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build()
        )
        return gen.generateKey()
    }

    override fun get(name: String): ByteArray? = try {
        val stored = prefs.getString(name, null) ?: return null
        val blob = Base64.decode(stored, Base64.NO_WRAP)
        val iv = blob.copyOfRange(0, 12)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, iv))
        cipher.doFinal(blob, 12, blob.size - 12)
    } catch (e: Exception) {
        null
    }

    override fun set(name: String, value: ByteArray): Boolean = try {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val out = cipher.iv + cipher.doFinal(value)
        prefs.edit().putString(name, Base64.encodeToString(out, Base64.NO_WRAP)).commit()
    } catch (e: Exception) {
        false
    }

    override fun delete(name: String): Boolean = prefs.edit().remove(name).commit()
}
