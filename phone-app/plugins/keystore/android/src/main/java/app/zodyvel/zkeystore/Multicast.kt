package app.zodyvel.zkeystore

import android.content.Context
import android.net.wifi.WifiManager

/** mDNS araması sürerken Wi-Fi çok yöne yayın kilidi (uygulama ve ekransız mod ortak). */
object Multicast {
    private var lock: WifiManager.MulticastLock? = null

    @Synchronized
    fun set(ctx: Context, on: Boolean) {
        if (on) {
            if (lock == null) {
                val wm = ctx.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
                lock = wm.createMulticastLock("zv-mdns").apply { setReferenceCounted(false) }
            }
            lock?.takeIf { !it.isHeld }?.acquire()
        } else {
            lock?.takeIf { it.isHeld }?.release()
        }
    }
}
