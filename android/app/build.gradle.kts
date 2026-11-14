// Aurora Player - Android application.
//
// The Rust core is built by scripts/android/build.sh and dropped into
// app/src/main/jniLibs; Gradle only packages it. The cargo task below re-runs the
// build for developers who have the NDK, and is skipped with -PskipRust when the
// .so files are already in place.
plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "app.aurora.player"
    compileSdk = 35

    defaultConfig {
        applicationId = "app.aurora.player"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        ndk {
            // The ABIs the Rust build script produces. arm64 is what a phone
            // needs; x86_64 is for the emulator.
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    buildFeatures {
        compose = true
    }
    packaging {
        jniLibs {
            // Keep the Rust .so uncompressed: nothing loads it before the app
            // starts, and extracting it costs startup time on older devices.
            useLegacyPackaging = false
        }
    }
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2024.09.03")
    implementation(composeBom)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.activity:activity-compose:1.9.2")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.6")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.6")
    debugImplementation("androidx.compose.ui:ui-tooling")
}

// `./gradlew assembleDebug` builds the Rust core first. On a machine without the
// NDK (this repository's CI sandbox, for one) pass -PskipRust and stage the .so
// files yourself; see scripts/android/README.md.
val cargoBuild = tasks.register<Exec>("cargoBuild") {
    workingDir = rootProject.projectDir.parentFile
    commandLine("bash", "scripts/android/build.sh", "--lib-only")
}

tasks.matching { it.name == "preBuild" }.configureEach {
    if (!project.hasProperty("skipRust")) {
        dependsOn(cargoBuild)
    }
}
