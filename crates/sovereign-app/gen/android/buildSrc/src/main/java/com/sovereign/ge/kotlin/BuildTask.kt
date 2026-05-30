import java.io.File
import org.apache.tools.ant.taskdefs.condition.Os
import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.logging.LogLevel
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.TaskAction

open class BuildTask : DefaultTask() {
    @Input
    var rootDirRel: String? = null
    @Input
    var target: String? = null
    @Input
    var release: Boolean? = null

    @TaskAction
    fun assemble() {
        if (Os.isFamily(Os.FAMILY_WINDOWS)) {
            // Original Windows path: spawn node directly against the npm-installed
            // @tauri-apps/cli, with Windows extension fallbacks.
            val executable = """C:\Program Files\nodejs\node"""
            try {
                runWithExecutable(executable, listOf("tauri", "android", "android-studio-script"))
            } catch (e: Exception) {
                val fallbacks = listOf("$executable.exe", "$executable.cmd", "$executable.bat")
                var lastException: Exception = e
                for (fallback in fallbacks) {
                    try {
                        runWithExecutable(fallback, listOf("tauri", "android", "android-studio-script"))
                        return
                    } catch (fallbackException: Exception) {
                        lastException = fallbackException
                    }
                }
                throw lastException
            }
        } else {
            // Linux/macOS: invoke the cargo-installed tauri CLI as `cargo tauri ...`.
            // Requires `cargo install tauri-cli --version '^2'` (or equivalent on PATH).
            runWithExecutable("cargo", listOf("tauri", "android", "android-studio-script"))
        }
    }

    fun runWithExecutable(executable: String, leadingArgs: List<String>) {
        val rootDirRel = rootDirRel ?: throw GradleException("rootDirRel cannot be null")
        val target = target ?: throw GradleException("target cannot be null")
        val release = release ?: throw GradleException("release cannot be null")

        project.exec {
            workingDir(File(project.projectDir, rootDirRel))
            executable(executable)
            args(leadingArgs)
            if (project.logger.isEnabled(LogLevel.DEBUG)) {
                args("-vv")
            } else if (project.logger.isEnabled(LogLevel.INFO)) {
                args("-v")
            }
            if (release) {
                args("--release")
            }
            args(listOf("--target", target))
        }.assertNormalExitValue()
    }
}