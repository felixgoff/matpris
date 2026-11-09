import groovy.json.JsonSlurper
import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("io.github.takahirom.roborazzi")
}

// One version for every Matpris app: the repo's package.json (`bun run bump patch`).
@Suppress("UNCHECKED_CAST")
val matprisVersion = (JsonSlurper().parse(file("../../package.json")) as Map<String, Any>)["version"] as String
val matprisVersionCode = matprisVersion.split('.', '-').take(3).fold(0) { n, part -> n * 100 + (part.toIntOrNull() ?: 0) }

// The release key, shared with the Tauri phone app: the Data Layer only pairs apps signed
// alike, and Android only installs an update over an app signed with the same key. The
// Release workflow sets ANDROID_KEYSTORE_FILE and the passwords from secrets; locally a
// keystore.properties in wear/ (storeFile, storePassword, keyAlias, keyPassword; gitignored)
// does the same. Without either, release builds are signed with the debug key so they
// still build, but they won't pair with or update over a properly signed copy.
val keystoreProperties = Properties().apply {
    val f = rootProject.file("keystore.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}
fun signingValue(env: String, prop: String): String? =
    System.getenv(env)?.takeIf { it.isNotEmpty() } ?: keystoreProperties.getProperty(prop)
val releaseStoreFile = signingValue("ANDROID_KEYSTORE_FILE", "storeFile")

android {
    namespace = "se.matpris.wear"
    compileSdk = 36

    defaultConfig {
        // Same id (and signing key) as the Tauri phone app: the Wear OS Data Layer only
        // connects a watch app to a phone app with the same id and signature.
        applicationId = "se.matpris.matpris"
        // Wear OS 3+; the Galaxy Watch 5 runs Wear OS 4 (API 33) and later.
        minSdk = 30
        targetSdk = 36
        versionCode = matprisVersionCode
        versionName = matprisVersion
    }

    signingConfigs {
        if (releaseStoreFile != null) {
            create("release") {
                storeFile = rootProject.file(releaseStoreFile)
                storePassword = signingValue("ANDROID_KEYSTORE_PASSWORD", "storePassword")
                keyAlias = signingValue("ANDROID_KEY_ALIAS", "keyAlias")
                // A PKCS12 keystore (keytool's default) has one password for both.
                keyPassword = signingValue("ANDROID_KEY_PASSWORD", "keyPassword") ?: storePassword
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = if (releaseStoreFile != null) {
                signingConfigs.getByName("release")
            } else {
                logger.warn("w: No release key (ANDROID_KEYSTORE_FILE or keystore.properties): the release build is signed with the debug key.")
                signingConfigs.getByName("debug")
            }
        }
    }
    buildFeatures {
        compose = true
    }
    // Screenshot tests draw the screens on a JVM (Robolectric): no emulator needed.
    testOptions {
        unitTests.isIncludeAndroidResources = true
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

dependencies {
    val wearCompose = "1.5.6"
    implementation("androidx.wear.compose:compose-material3:$wearCompose")
    implementation("androidx.wear.compose:compose-foundation:$wearCompose")
    implementation("androidx.wear.compose:compose-navigation:$wearCompose")
    implementation("androidx.wear:wear-input:1.2.0")
    implementation("androidx.activity:activity-compose:1.11.0")
    implementation("androidx.core:core-splashscreen:1.2.0")
    implementation("com.google.android.gms:play-services-location:21.3.0")
    implementation("com.google.android.gms:play-services-wearable:19.0.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-play-services:1.10.2")

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.16")
    testImplementation("io.github.takahirom.roborazzi:roborazzi-compose:1.78.0")
    testImplementation(platform("androidx.compose:compose-bom:2025.12.01"))
    testImplementation("androidx.compose.ui:ui-test-junit4")
    debugImplementation(platform("androidx.compose:compose-bom:2025.12.01"))
    debugImplementation("androidx.compose.ui:ui-test-manifest")
    // Android's org.json is a stub in local unit tests.
    testImplementation("org.json:json:20250517")
}
