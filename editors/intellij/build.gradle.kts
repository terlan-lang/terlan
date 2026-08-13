plugins {
    kotlin("jvm") version "2.3.21"
    id("org.jetbrains.intellij.platform") version "2.18.1"
}

group = "org.terlan"
version = "0.0.7"

repositories {
    mavenCentral()
    intellijPlatform {
        defaultRepositories()
    }
}

dependencies {
    intellijPlatform {
        // JetBrains' LSP API is an Ultimate-platform module for the 2024.3
        // baseline. Compiling against Community made the previous descriptor
        // impossible to register or execute.
        intellijIdeaUltimate("2024.3")
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
