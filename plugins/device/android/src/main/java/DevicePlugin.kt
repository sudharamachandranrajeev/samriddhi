package dev.brahma.samriddhi.device

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.view.WindowManager
import android.webkit.WebView
import android.provider.Settings
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import androidx.core.content.FileProvider
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.KeyStore
import java.security.MessageDigest
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

@InvokeArg
class SecureArgs {
    var on: Boolean = false
}

@InvokeArg
class AiKeyArgs {
    var key: String? = null
}

@InvokeArg
class FetchArgs {
    var url: String = ""
}

@InvokeArg
class ApkArgs {
    var apkUrl: String = ""
    var sumsUrl: String = ""
    var name: String = ""
}

// The database key is random, and is stored only wrapped (AES-GCM) by a key
// that never leaves the Android Keystore. Adapted from Arogya Swasthya.
@TauriPlugin
class DevicePlugin(private val activity: Activity) : Plugin(activity) {
    private val prefs = activity.getSharedPreferences("samriddhi-device", Context.MODE_PRIVATE)
    // Each secret has its own Keystore key and its own slot, so the AI key and
    // the database key never share a wrapping key.
    private val dbAlias = "dev.brahma.samriddhi.db-wrap"
    private val aiAlias = "dev.brahma.samriddhi.ai-wrap"

    private fun wrapKey(alias: String): SecretKey {
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

    private fun seal(alias: String, slot: String, text: String, charset: java.nio.charset.Charset) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, wrapKey(alias))
        val sealed = cipher.iv + cipher.doFinal(text.toByteArray(charset))
        // commit, not apply: the secret must be on disk before anything relies on it.
        check(prefs.edit().putString(slot, Base64.encodeToString(sealed, Base64.NO_WRAP)).commit())
    }

    private fun unseal(alias: String, slot: String, charset: java.nio.charset.Charset): String? {
        val sealed = prefs.getString(slot, null)?.let { Base64.decode(it, Base64.NO_WRAP) } ?: return null
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, wrapKey(alias), GCMParameterSpec(128, sealed, 0, 12))
        return String(cipher.doFinal(sealed, 12, sealed.size - 12), charset)
    }

    private fun store(hex: String) = seal(dbAlias, "wrapped", hex, Charsets.US_ASCII)

    private fun load(): String? = unseal(dbAlias, "wrapped", Charsets.US_ASCII)

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

    // The user's own AI API key: encrypted by its own Keystore key. Errors
    // never include the key itself.
    @Command
    fun aiKey(invoke: Invoke) {
        try {
            val result = JSObject()
            unseal(aiAlias, "ai-wrapped", Charsets.UTF_8)?.let { result.put("key", it) }
            invoke.resolve(result)
        } catch (e: Exception) {
            invoke.reject("the saved AI key cannot be read")
        }
    }

    @Command
    fun setAiKey(invoke: Invoke) {
        val key = invoke.parseArgs(AiKeyArgs::class.java).key
        if (key.isNullOrEmpty() || key.length > 512) {
            invoke.reject("an API key is 1 to 512 characters")
            return
        }
        try {
            seal(aiAlias, "ai-wrapped", key, Charsets.UTF_8)
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("could not store the AI key")
        }
    }

    // Removing needs no decryption, so it also clears a key that can no longer be read.
    @Command
    fun clearAiKey(invoke: Invoke) {
        if (prefs.edit().remove("ai-wrapped").commit()) invoke.resolve()
        else invoke.reject("could not remove the AI key")
    }

    // FLAG_SECURE blocks screenshots, screen recording and the recents
    // thumbnail. The choice is mirrored in a plain preference so it is applied
    // as soon as the plugin loads (and again if Android remakes the window),
    // before the app's own setting is read from the encrypted store.
    private fun applySecure(on: Boolean) {
        activity.runOnUiThread {
            if (on) activity.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
            else activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        }
    }

    override fun load(webView: WebView) {
        super.load(webView)
        applySecure(prefs.getBoolean("blockCapture", false))
    }

    @Command
    fun setSecure(invoke: Invoke) {
        val on = invoke.parseArgs(SecureArgs::class.java).on
        prefs.edit().putBoolean("blockCapture", on).apply()
        applySecure(on)
        invoke.resolve()
    }

    // Best-effort and trivially bypassable: informational only, never a gate.
    private fun findRootHints(): List<String> {
        val hints = mutableListOf<String>()
        try {
            val dirs = (System.getenv("PATH")?.split(":") ?: emptyList()) + listOf("/system/bin", "/system/xbin", "/sbin")
            if (dirs.distinct().any { File(it, "su").exists() }) hints.add("su binary present")
            if (Build.TAGS?.contains("test-keys") == true) hints.add("test-keys build")
            val managers = listOf(
                "/system/app/Superuser.apk",
                "/system/app/SuperSU.apk",
                "/data/adb/magisk",
                "/sbin/.magisk",
            )
            if (managers.any { File(it).exists() }) hints.add("root manager files present")
        } catch (e: Exception) {
            // Cannot check: say nothing rather than guess.
        }
        return hints
    }

    @Command
    fun rootHints(invoke: Invoke) {
        val result = JSObject()
        result.put("hints", org.json.JSONArray(findRootHints()))
        invoke.resolve(result)
    }

    private fun open(intent: Intent): Boolean =
        try {
            activity.startActivity(intent)
            true
        } catch (e: ActivityNotFoundException) {
            false
        }

    // Updates come only from Samriddhi's own GitHub releases, over HTTPS.
    private val releases = "https://github.com/sudharamachandranrajeev/samriddhi-wealth-manager/releases/"

    private fun get(url: String): HttpURLConnection {
        require(url.startsWith(releases)) { "not a Samriddhi release address" }
        val c = URL(url).openConnection() as HttpURLConnection
        c.connectTimeout = 15_000
        c.readTimeout = 60_000
        c.setRequestProperty("User-Agent", "Samriddhi")
        if (c.responseCode != 200) throw IllegalStateException("the server answered ${c.responseCode}")
        return c
    }

    @Command
    fun fetchText(invoke: Invoke) {
        val args = invoke.parseArgs(FetchArgs::class.java)
        Thread {
            try {
                val text = get(args.url).inputStream.use { it.readBytes().toString(Charsets.UTF_8) }
                val result = JSObject()
                result.put("text", text)
                invoke.resolve(result)
            } catch (e: Exception) {
                invoke.reject("could not reach the update server: ${e.message}")
            }
        }.start()
    }

    // Download the APK, check it against SHA256SUMS.txt from the same release,
    // and hand it to Android's installer. Android itself refuses an APK that is
    // not signed with the same key as the installed app.
    @Command
    fun installApk(invoke: Invoke) {
        val args = invoke.parseArgs(ApkArgs::class.java)
        if (!args.name.matches(Regex("[A-Za-z0-9._-]+\\.apk"))) {
            invoke.reject("odd file name")
            return
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && !activity.packageManager.canRequestPackageInstalls()) {
            open(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:" + activity.packageName)))
            invoke.reject("Allow Samriddhi to install updates in the page that opened, then try again.")
            return
        }
        Thread {
            val dir = File(activity.filesDir, "updates").apply { mkdirs() }
            val file = File(dir, args.name)
            try {
                val sums = get(args.sumsUrl).inputStream.use { it.readBytes().toString(Charsets.UTF_8) }
                val want = sums.lineSequence()
                    .map { it.trim().split(Regex("\\s+")) }
                    .firstOrNull { it.size == 2 && it[1].removePrefix("*") == args.name }
                    ?.get(0)?.lowercase()
                    ?: throw IllegalStateException("${args.name} is not listed in SHA256SUMS.txt")
                dir.listFiles()?.forEach { it.delete() }
                val digest = MessageDigest.getInstance("SHA-256")
                get(args.apkUrl).inputStream.use { input ->
                    file.outputStream().use { out ->
                        val buf = ByteArray(64 * 1024)
                        while (true) {
                            val n = input.read(buf)
                            if (n < 0) break
                            digest.update(buf, 0, n)
                            out.write(buf, 0, n)
                        }
                    }
                }
                val got = digest.digest().joinToString("") { "%02x".format(it) }
                if (got != want) {
                    file.delete()
                    throw IllegalStateException("the download is damaged (checksum does not match)")
                }
                val uri = FileProvider.getUriForFile(activity, activity.packageName + ".samriddhi.updates", file)
                val intent = Intent(Intent.ACTION_VIEW)
                    .setDataAndType(uri, "application/vnd.android.package-archive")
                    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
                activity.runOnUiThread { open(intent) }
                invoke.resolve()
            } catch (e: Exception) {
                file.delete()
                invoke.reject("the update could not be downloaded: ${e.message}")
            }
        }.start()
    }
}
