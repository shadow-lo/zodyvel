package app.zodyvel.zkeystore

import android.Manifest
import android.app.AlarmManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.drawable.Icon
import android.os.Build
import androidx.core.content.ContextCompat
import org.json.JSONArray
import java.io.File
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId

/**
 * G3 · Ajanda hatırlatıcıları. Rust listeyi süzer (başlık ≤ 80, ön süre ≤ 1440 dk, en çok 200); burada AlarmManager'a kurulur.
 * Liste diske yalnız Keystore (zv_wrap) ile şifreli yazılır. Alarm intent'ine başlık konmaz, yalnız anahtar.
 */
object Reminders {
    const val CH = "zv_agenda"
    private const val FILE = "reminders.bin"
    private const val TAG = "zv_agenda"

    class Item(val key: Int, val date: String, val time: String, val lead: Int, val title: String)

    private fun parse(json: String): List<Item> {
        val a = JSONArray(json)
        val out = ArrayList<Item>()
        for (i in 0 until a.length()) {
            val o = a.optJSONObject(i) ?: continue
            out.add(Item(o.getInt("key"), o.getString("date"), o.getString("time"), o.optInt("lead", 0), o.optString("title", "")))
        }
        return out
    }

    private fun file(ctx: Context) = File(ctx.filesDir, FILE)

    private fun load(ctx: Context): List<Item> {
        val f = file(ctx)
        if (!f.exists()) return emptyList()
        return try {
            parse(String(Vault.unwrap(f.readBytes()), Charsets.UTF_8))
        } catch (e: Exception) {
            emptyList()
        }
    }

    private fun pending(ctx: Context, key: Int, create: Boolean): PendingIntent? {
        val i = Intent(ctx, ReminderReceiver::class.java).putExtra("key", key)
        val flags = PendingIntent.FLAG_IMMUTABLE or (if (create) PendingIntent.FLAG_UPDATE_CURRENT else PendingIntent.FLAG_NO_CREATE)
        return PendingIntent.getBroadcast(ctx, key, i, flags)
    }

    private fun cancelAll(ctx: Context) {
        val am = ctx.getSystemService(Context.ALARM_SERVICE) as AlarmManager
        for (item in load(ctx)) {
            pending(ctx, item.key, false)?.let { p ->
                am.cancel(p)
                p.cancel()
            }
        }
    }

    private fun arm(ctx: Context, items: List<Item>) {
        val am = ctx.getSystemService(Context.ALARM_SERVICE) as AlarmManager
        val now = System.currentTimeMillis()
        for (item in items) {
            try {
                val at = LocalDate.parse(item.date).atTime(LocalTime.parse(item.time))
                    .atZone(ZoneId.systemDefault()).minusMinutes(item.lead.toLong()).toInstant().toEpochMilli()
                if (at <= now) continue
                val p = pending(ctx, item.key, true) ?: continue
                if (Build.VERSION.SDK_INT >= 31 && !am.canScheduleExactAlarms()) {
                    am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, p)
                } else {
                    am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, p)
                }
            } catch (e: Exception) {
            }
        }
    }

    @Synchronized
    fun sync(ctx: Context, json: String) {
        val c = ctx.applicationContext
        // A-D4: önce ayrıştır; JSON bozuksa burada hata fırlar, eski alarmlar ve eski dosya birlikte yerinde kalır.
        val items = parse(json)
        cancelAll(c)
        if (items.isEmpty()) {
            file(c).delete()
            return
        }
        file(c).writeBytes(Vault.wrap(json.toByteArray(Charsets.UTF_8)))
        arm(c, items)
    }

    @Synchronized
    fun rearm(ctx: Context) {
        val c = ctx.applicationContext
        arm(c, load(c))
    }

    fun find(ctx: Context, key: Int): Item? = load(ctx).firstOrNull { it.key == key }

    fun ensureChannel(ctx: Context) {
        val m = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        m.createNotificationChannel(NotificationChannel(CH, "Ajanda hatırlatıcıları", NotificationManager.IMPORTANCE_HIGH).apply {
            description = "Planlı işlerin hatırlatması"
            lockscreenVisibility = Notification.VISIBILITY_PRIVATE
        })
    }

    fun show(ctx: Context, item: Item) {
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(ctx, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) return
        ensureChannel(ctx)
        val m = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val icon = Icon.createWithResource(ctx, R.drawable.ic_zv_notif)
        val pub = Notification.Builder(ctx, CH).setSmallIcon(icon)
            .setContentTitle("Zodyvel").setContentText("1 hatırlatma")
            .setVisibility(Notification.VISIBILITY_PUBLIC).build()
        val text = if (item.lead > 0) "${item.time} · ${item.lead} dk sonra" else "${item.time} · şimdi"
        val b = Notification.Builder(ctx, CH).setSmallIcon(icon)
            .setContentTitle(item.title)
            .setContentText(text)
            .setCategory(Notification.CATEGORY_REMINDER)
            .setAutoCancel(true)
            .setVisibility(Notification.VISIBILITY_PRIVATE)
            .setPublicVersion(pub)
        ctx.packageManager.getLaunchIntentForPackage(ctx.packageName)?.let {
            it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            b.setContentIntent(PendingIntent.getActivity(ctx, 2, it, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT))
        }
        // Etiketli bildirim: Notifier'ın bildirimleriyle kimlik çakışması olmaz.
        m.notify(TAG, item.key, b.build())
    }
}

class ReminderReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val key = intent.getIntExtra("key", Int.MIN_VALUE)
        if (key == Int.MIN_VALUE) return
        try {
            Reminders.find(context, key)?.let { Reminders.show(context, it) }
        } catch (e: Exception) {
        }
    }
}

class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        // A-D3: alıcı dışa açık; beklenmeyen eylemde hiçbir iş yapmadan çık (başka uygulama tetikleyip pil harcatamasın).
        // Aşağıdaki eylemlerin hepsi korumalı sistem yayını: başka uygulama bunları gönderemez, gönderebildikleri burada elenir.
        // ACTION_TIME_CHANGED = "android.intent.action.TIME_SET".
        val a = intent.action
        val boot = a == Intent.ACTION_BOOT_COMPLETED || a == Intent.ACTION_MY_PACKAGE_REPLACED
        val time = a == Intent.ACTION_TIMEZONE_CHANGED || a == Intent.ACTION_TIME_CHANGED
        // A-D5: kesin alarm izni değişti (API 31-32; 33+'da USE_EXACT_ALARM geri alınamaz). İzin verildiyse alarmlar
        // kesin olarak, değilse arm() içindeki yedek yolla yeniden kurulur.
        val exact = Build.VERSION.SDK_INT >= 31 && a == AlarmManager.ACTION_SCHEDULE_EXACT_ALARM_PERMISSION_STATE_CHANGED
        if (!boot && !time && !exact) return
        try {
            Reminders.rearm(context)
        } catch (e: Exception) {
        }
        if (!boot) return
        try {
            val dir = Vault.coreDir(context) ?: return
            if (File(dir, "boot.on").exists() && File(dir, "pair.json").exists() && !File(dir, "live.off").exists()) {
                Notifier.ensureChannels(context)
                ContextCompat.startForegroundService(context, Intent(context, BridgeService::class.java).putExtra("headless", true))
            }
        } catch (e: Exception) {
        }
    }
}
