# shellcheck shell=bash
#
# Sourced by build-java.sh and run-integration-tests-android.sh so both Gradle
# invocations use the same JDK.
#
# Prints a JDK home for the Android Gradle build: $JAVA_HOME if it is set and
# exists, otherwise a JDK 17 or 21 from the platform's usual locations. Prints
# nothing if none is found. Gradle 8.9 cannot run on newer JDKs, so the system
# default java is not used.
find_java_home() {
    if [ -n "${JAVA_HOME:-}" ] && [ -d "$JAVA_HOME" ]; then
        echo "$JAVA_HOME"
        return
    fi

    local candidate
    case "$(uname -s)" in
        Darwin)
            local v brew_jdk
            for v in 17 21; do
                candidate="$(/usr/libexec/java_home -v "$v" 2>/dev/null || true)"
                if [ -n "$candidate" ] && [ -d "$candidate" ]; then
                    echo "$candidate"
                    return
                fi
            done
            for v in 17 21 11; do
                brew_jdk="$(brew --prefix "openjdk@$v" 2>/dev/null || true)"
                if [ -n "$brew_jdk" ] && [ -d "$brew_jdk/libexec/openjdk.jdk/Contents/Home" ]; then
                    echo "$brew_jdk/libexec/openjdk.jdk/Contents/Home"
                    return
                fi
            done
            ;;
        Linux)
            for candidate in \
                /usr/lib/jvm/java-17-openjdk-amd64 \
                /usr/lib/jvm/java-17-openjdk-arm64 \
                /usr/lib/jvm/java-17-openjdk \
                /usr/lib/jvm/java-17 \
                /usr/lib/jvm/java-21-openjdk-amd64 \
                /usr/lib/jvm/java-21-openjdk-arm64 \
                /usr/lib/jvm/java-21-openjdk \
                /usr/lib/jvm/java-21 \
                /usr/lib/jvm/default-java; do
                if [ -d "$candidate" ]; then
                    echo "$candidate"
                    return
                fi
            done
            ;;
    esac
}
