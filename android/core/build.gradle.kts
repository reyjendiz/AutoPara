import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("org.jetbrains.kotlin.jvm")
}

// Plain JVM module: no Android classes, so the parser and scheduling rules are unit-tested on any
// machine. The app module depends on it.
java {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
}

kotlin {
    compilerOptions { jvmTarget.set(JvmTarget.JVM_17) }
}

dependencies {
    testImplementation(kotlin("test"))
    testImplementation("junit:junit:4.13.2")
    // Reads the golden JSON in tests only; the shipped module has no JSON dependency.
    testImplementation("org.json:json:20240303")
}

tasks.test {
    useJUnit()
    testLogging { events("failed", "skipped"); exceptionFormat = org.gradle.api.tasks.testing.logging.TestExceptionFormat.FULL }
}
