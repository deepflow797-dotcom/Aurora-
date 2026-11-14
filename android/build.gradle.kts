// Top-level build file. Versions are pinned rather than ranged: this project
// builds offline in the sandbox, and an unpinned plugin would turn a reproducible
// build into a download.
plugins {
    id("com.android.application") version "8.7.0" apply false
    id("org.jetbrains.kotlin.android") version "2.0.20" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.0.20" apply false
}
