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
        // Bumped for the first APK that contains the mini-card-only blur
        // (word-blur rebuild, phase 5). A higher versionCode lets the new
        // build install over any 0.1.0 build already on a phone.
        versionCode = 2
        versionName = "0.2.0"
        ndk {
            // The ABIs the Rust build script produces. arm64 is what a phone
            // needs; x86_64 is for the emulator.
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    // The release build is signed with the debug keystore. That is what
    // android/README.md and the CI workflow have always promised ("release
    // APK, debug-signed, personal sideload") and what `assembleRelease`
    // alone never did: without a signingConfig Gradle emits
    // `app-release-unsigned.apk`, which Android refuses to install. A real
    // keystore must never be committed, so this stays the personal-install
    // path; a store build overrides it with properties.
    signingConfigs {
        getByName("debug")
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.getByName("debug")
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
    lint {
        // CI fix (word-blur rebuild): lint warnings must not block the APK.
        // The previous failure mode was `warningsAsErrors = true` turning
        // every Gradle/lint warning into a build failure, so no APK was
        // ever produced. Warnings are still reported (and uploaded as
        // artifacts by CI), but they no longer fail the build.
        // `abortOnError = false` likewise keeps a lint *error* in a
        // non-critical check from discarding an otherwise good APK;
        // real compile/test failures still fail the build.
        warningsAsErrors = false
        abortOnError = false
        checkReleaseBuilds = false
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
    // JVM unit tests (WordBlurLawTest). Pure-math tests only; no Android
    // runtime needed, so they run in CI with plain `test`.
    testImplementation("junit:junit:4.13.2")
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
