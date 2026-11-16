import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// A-O2: release imzası yalnız ayrı bir keystore ile. Yolu ZV_KEYSTORE_PROPS ortam değişkeninden ya da aynı adlı gradle
// özelliğinden (-PZV_KEYSTORE_PROPS=... / ~/.gradle/gradle.properties) gelen bir keystore.properties dosyası verir;
// dosya OneDrive dışında durmalı. İçeriği: storeFile, storePassword, keyAlias, keyPassword. Dosya yoksa release
// derlemesi açık bir hatayla durur; debug anahtarına asla düşülmez. Debug derlemeleri bundan etkilenmez.
class ReleaseKey(val storeFile: File, val storePassword: String, val keyAlias: String, val keyPassword: String)

/** (anahtar, hata): ikisinden biri dolu. */
fun loadReleaseKey(): Pair<ReleaseKey?, String?> {
    val path = (System.getenv("ZV_KEYSTORE_PROPS") ?: (findProperty("ZV_KEYSTORE_PROPS") as String?))?.trim()
    if (path.isNullOrEmpty()) {
        return Pair(null, "ZV_KEYSTORE_PROPS (ortam değişkeni ya da gradle özelliği) keystore.properties dosyasını göstermeli")
    }
    val propsFile = File(path)
    if (!propsFile.isFile) return Pair(null, "ZV_KEYSTORE_PROPS ile verilen dosya bulunamadı: $path")
    val p = Properties().apply { propsFile.inputStream().use { load(it) } }
    val need = listOf("storeFile", "storePassword", "keyAlias", "keyPassword")
    val missing = need.filter { p.getProperty(it).isNullOrBlank() }
    if (missing.isNotEmpty()) return Pair(null, "keystore.properties içinde eksik alan: ${missing.joinToString(", ")}")
    // Göreli storeFile, properties dosyasının klasörüne göre çözülür.
    val raw = File(p.getProperty("storeFile").trim())
    val store = if (raw.isAbsolute) raw else File(propsFile.absoluteFile.parentFile, raw.path)
    if (!store.isFile) return Pair(null, "keystore.properties içindeki storeFile bulunamadı: ${store.path}")
    return Pair(ReleaseKey(store, p.getProperty("storePassword"), p.getProperty("keyAlias").trim(), p.getProperty("keyPassword")), null)
}

val releaseKeyResult = loadReleaseKey()
val releaseKey: ReleaseKey? = releaseKeyResult.first
val releaseKeystoreError: String? = releaseKeyResult.second

android {
    compileSdk = 37
    namespace = "app.zodyvel.phone"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "app.zodyvel.phone"
        minSdk = 30
        // A-Y1: Android 17 (API 37) hedefleyen uygulamada yerel ağ erişimi ayrı izin istiyor; izin akışı yazılana
        // dek 36 hedeflenir (köprü ve mDNS sessizce kesilmesin). compileSdk 37 kalabilir.
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        // Yalnız dosya eksiksizse tanımlanır; yoksa release imzasız kalır ve pre*ReleaseBuild aşağıda hata verir.
        releaseKey?.let { k ->
            create("release") {
                storeFile = k.storeFile
                storePassword = k.storePassword
                keyAlias = k.keyAlias
                keyPassword = k.keyPassword
            }
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            // G4: günlük kullanım için hata ayıklanamayan sürüm (run-as / WebView uzaktan hata ayıklama kapalı,
            // düz HTTP kapalı). A-O2: ayrı release anahtarıyla imzalanır (yukarıda); debug anahtarı kullanılmaz.
            isDebuggable = false
            signingConfig = signingConfigs.findByName("release")
            optimization {
               enable = true
            }
            proguardFiles(
                *fileTree(".") {
                  include("**/*.pro")
                  exclude("build/**")
                }.files.toTypedArray()
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    buildFeatures {
        buildConfig = true
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

rust {
    rootDirRel = "../../../"
}

// A-O2: keystore.properties yok ya da eksikse her release derlemesi (tüm ABI/flavor'lar) en başta durur.
tasks.configureEach {
    if (releaseKeystoreError != null && name.startsWith("pre") && name.endsWith("ReleaseBuild")) {
        val msg = releaseKeystoreError
        doFirst { throw GradleException("Release imzalanamıyor: $msg (debug anahtarına düşülmez.)") }
    }
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = file("tauri.build.gradle.kts"))
