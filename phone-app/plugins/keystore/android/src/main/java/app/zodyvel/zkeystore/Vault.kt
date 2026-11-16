package app.zodyvel.zkeystore

import android.app.KeyguardManager
import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Eklenti örneği olmadan (ekransız açılış, alarm alıcıları) kullanılabilen Keystore yardımcıları. Anahtar ve günlük kuralları KeystorePlugin ile aynı. */
object Vault {
    const val SIGN_ALIAS = "zv_sign"
    const val WRAP_ALIAS = "zv_wrap"

    fun hex(b: ByteArray) = b.joinToString("") { "%02x".format(it) }
    fun unhex(s: String): ByteArray = ByteArray(s.length / 2) { s.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
    fun keystore(): KeyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

    fun deviceSecure(ctx: Context): Boolean =
        (ctx.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager).isDeviceSecure

    fun deviceName(): String = Build.MODEL ?: "Telefon"

    @Synchronized
    private fun wrapKey(): SecretKey {
        val store = keystore()
        (store.getKey(WRAP_ALIAS, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(WRAP_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build()
        )
        return gen.generateKey()
    }

    /** AES-256-GCM; çıktı = 12 bayt IV + şifreli metin. */
    fun wrap(data: ByteArray): ByteArray {
        val c = Cipher.getInstance("AES/GCM/NoPadding")
        c.init(Cipher.ENCRYPT_MODE, wrapKey())
        return c.iv + c.doFinal(data)
    }

    fun unwrap(raw: ByteArray): ByteArray {
        require(raw.size >= 13)
        val c = Cipher.getInstance("AES/GCM/NoPadding")
        c.init(Cipher.DECRYPT_MODE, wrapKey(), GCMParameterSpec(128, raw.copyOfRange(0, 12)))
        return c.doFinal(raw.copyOfRange(12, raw.size))
    }

    /**
     * A-D6: İmza anahtarı iki ad arasında döner. Keystore'da yeniden adlandırma olmadığı için yeni anahtar boştaki adla
     * üretilir; başarılı olunca etkin ad tercihlerde değiştirilir, eskisi ancak ondan sonra silinir.
     */
    const val SIGN_ALIAS_ALT = "zv_sign_b"
    private const val PREF_SIGN = "sign_alias"

    fun signAlias(ctx: Context): String =
        if (prefs(ctx).getString(PREF_SIGN, SIGN_ALIAS) == SIGN_ALIAS_ALT) SIGN_ALIAS_ALT else SIGN_ALIAS

    fun spareSignAlias(ctx: Context): String = if (signAlias(ctx) == SIGN_ALIAS) SIGN_ALIAS_ALT else SIGN_ALIAS

    /** Eşzamanlı yazılır (commit): ad değişmeden eski anahtar silinmesin. */
    fun setSignAlias(ctx: Context, alias: String): Boolean = prefs(ctx).edit().putString(PREF_SIGN, alias).commit()

    fun prefs(ctx: Context) = ctx.getSharedPreferences("zv", Context.MODE_PRIVATE)
    fun coreDir(ctx: Context): String? = prefs(ctx).getString("core_dir", null)?.takeIf { it.isNotEmpty() }
}
