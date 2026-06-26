plugins {
    kotlin("jvm") version "2.0.21"
    id("org.jetbrains.intellij.platform") version "2.2.1"
}

group = "org.terlan"
version = "0.0.5"

repositories {
    mavenCentral()
    intellijPlatform {
        defaultRepositories()
    }
}

dependencies {
    intellijPlatform {
        intellijIdeaCommunity("2024.3")
        bundledPlugin("com.intellij.modules.platform")
    }
}

intellijPlatform {
    pluginConfiguration {
        name = "Terlan"
        ideaVersion {
            sinceBuild = "243"
        }
    }
}
