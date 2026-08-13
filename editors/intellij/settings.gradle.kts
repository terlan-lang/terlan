pluginManagement {
    repositories {
        gradlePluginPortal()
        mavenCentral()
    }
}

dependencyResolutionManagement {
    // The IntelliJ Platform Gradle plugin contributes JetBrains repositories
    // from the project dependency block. Rejecting all project repositories
    // made every real Gradle build fail before dependency resolution began.
    repositoriesMode.set(RepositoriesMode.PREFER_PROJECT)
}

rootProject.name = "terlan-intellij"
