package dev.brahma.samriddhi.device

import android.app.Activity
import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

@InvokeArg
class KeyArgs {
    var mayCreate: Boolean = false
    var key: String? = null
}

// The database key is random, and is stored only wrapped (AES-GCM) by a key
// that never leaves the Android Keystore. Adapted from Arogya Swasthya.
@TauriPlugin
class DevicePlugin(private val activity: Activity) : Plugin(activity) {
    private val prefs = activity.getSharedPreferences("samriddhi-device", Context.MODE_PRIVATE)
    private val alias = "dev.brahma.samriddhi.db-wrap"

    private fun wrapKey(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(alias, null) as? SecretKey)?.let { return it }
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

    private fun store(hex: String) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, wrapKey())
        val sealed = cipher.iv + cipher.doFinal(hex.toByteArray(Charsets.US_ASCII))
        // commit, not apply: the key must be on disk before the database is made.
        check(prefs.edit().putString("wrapped", Base64.encodeToString(sealed, Base64.NO_WRAP)).commit())
    }

    private fun load(): String? {
        val sealed = prefs.getString("wrapped", null)?.let { Base64.decode(it, Base64.NO_WRAP) } ?: return null
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, wrapKey(), GCMParameterSpec(128, sealed, 0, 12))
        return String(cipher.doFinal(sealed, 12, sealed.size - 12), Charsets.US_ASCII)
    }

    @Command
    fun dbKey(invoke: Invoke) {
        val args = invoke.parseArgs(KeyArgs::class.java)
        try {
            val result = JSObject()
            val existing = load()
            if (existing != null) {
                result.put("key", existing)
                result.put("created", false)
            } else if (args.mayCreate) {
                val raw = ByteArray(32).also { SecureRandom().nextBytes(it) }
                val hex = raw.joinToString("") { "%02x".format(it) }
                store(hex)
                result.put("key", hex)
                result.put("created", true)
            } else {
                result.put("created", false)
            }
            invoke.resolve(result)
        } catch (e: Exception) {
            // Never replace a key that cannot be read: the database would be lost.
            invoke.reject("the stored database key cannot be read: ${e.message}")
        }
    }

    @Command
    fun setDbKey(invoke: Invoke) {
        val args = invoke.parseArgs(KeyArgs::class.java)
        val hex = args.key
        if (hex == null || hex.length != 64) {
            invoke.reject("a key is 64 hex digits")
            return
        }
        try {
            store(hex)
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("could not store the key: ${e.message}")
        }
    }
}
