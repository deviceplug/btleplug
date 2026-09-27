#!/usr/bin/env bash
#
# Build the Java/Android portion of btleplug (src/droidplug/java).
#
# Checks for and installs Java (via Homebrew on macOS) if necessary,
# locates the Android SDK, then runs the Gradle build.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
JAVA_DIR="$PROJECT_ROOT/src/droidplug/java"

# shellcheck source=lib/java-home.sh
source "$SCRIPT_DIR/lib/java-home.sh"

# --- Colors (if terminal) ---------------------------------------------------
if [ -t 1 ]; then
    RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[0;33m'; NC='\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; NC=''
fi

info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }
die()   { error "$@"; exit 1; }

# --- Detect OS ---------------------------------------------------------------
OS="$(uname -s)"

# --- Java --------------------------------------------------------------------
ensure_java() {
    if command -v java &>/dev/null && java -version &>/dev/null 2>&1; then
        local ver
        ver="$(java -version 2>&1 | head -1 | sed -E 's/.*"([0-9]+).*/\1/')"
        if [ "$ver" -ge 11 ] 2>/dev/null; then
            info "Java $ver found: $(command -v java)"
            return 0
        else
            warn "Java $ver found but AGP 7.4 requires Java >= 11"
        fi
    fi

    info "Java not found or too old — attempting to install..."

    case "$OS" in
        Darwin)
            if ! command -v brew &>/dev/null; then
                die "Homebrew not found. Install it from https://brew.sh or install Java 11+ manually."
            fi
            info "Installing openjdk@17 via Homebrew..."
            brew install openjdk@17
            # Homebrew installs openjdk to its prefix; create the system symlink
            # so /usr/libexec/java_home can find it. This may fail without sudo
            # but we can still use it directly via JAVA_HOME.
            local jdk_path
            jdk_path="$(brew --prefix openjdk@17)/libexec/openjdk.jdk/Contents/Home"
            if [ -d "$jdk_path" ]; then
                export JAVA_HOME="$jdk_path"
                export PATH="$JAVA_HOME/bin:$PATH"
            else
                die "openjdk@17 installed but JDK home not found at expected path."
            fi
            ;;
        Linux)
            if command -v apt-get &>/dev/null; then
                info "Installing openjdk-17-jdk via apt..."
                sudo apt-get update -qq && sudo apt-get install -y -qq openjdk-17-jdk
            elif command -v dnf &>/dev/null; then
                info "Installing java-17-openjdk-devel via dnf..."
                sudo dnf install -y java-17-openjdk-devel
            elif command -v pacman &>/dev/null; then
                info "Installing jdk17-openjdk via pacman..."
                sudo pacman -S --noconfirm jdk17-openjdk
            else
                die "Could not detect package manager. Install Java 11+ manually."
            fi
            ;;
        *)
            die "Unsupported OS '$OS'. Install Java 11+ manually."
            ;;
    esac

    # Verify
    if ! command -v java &>/dev/null || ! java -version &>/dev/null 2>&1; then
        die "Java installation failed or 'java' is not on PATH."
    fi
    info "Java installed successfully."
}

# --- JAVA_HOME ---------------------------------------------------------------
ensure_java_home() {
    JAVA_HOME="$(find_java_home)"
    if [ -z "$JAVA_HOME" ]; then
        die "Could not find a JDK 17 or 21. Set JAVA_HOME and re-run."
    fi
    export JAVA_HOME
    info "JAVA_HOME=$JAVA_HOME"
}

# --- Android SDK -------------------------------------------------------------
ensure_android_sdk() {
    if [ -n "${ANDROID_HOME:-}" ] && [ -d "$ANDROID_HOME" ]; then
        info "ANDROID_HOME=$ANDROID_HOME"
        return 0
    fi

    # Check ANDROID_SDK_ROOT (older env var)
    if [ -n "${ANDROID_SDK_ROOT:-}" ] && [ -d "$ANDROID_SDK_ROOT" ]; then
        export ANDROID_HOME="$ANDROID_SDK_ROOT"
        info "ANDROID_HOME=$ANDROID_HOME (from ANDROID_SDK_ROOT)"
        return 0
    fi

    # Common default locations
    local candidates=()
    case "$OS" in
        Darwin) candidates=("$HOME/Library/Android/sdk") ;;
        Linux)  candidates=("$HOME/Android/Sdk" "$HOME/android-sdk") ;;
    esac

    for candidate in "${candidates[@]}"; do
        if [ -d "$candidate" ]; then
            export ANDROID_HOME="$candidate"
            info "ANDROID_HOME=$ANDROID_HOME (auto-detected)"
            return 0
        fi
    done

    die "Android SDK not found. Set ANDROID_HOME and re-run, or install via Android Studio / sdkmanager."
}

# --- Write local.properties --------------------------------------------------
write_local_properties() {
    local props="$JAVA_DIR/local.properties"
    # Gradle Android plugin requires sdk.dir in local.properties
    echo "sdk.dir=$ANDROID_HOME" > "$props"
    info "Wrote $props"
}

# --- Build -------------------------------------------------------------------
run_gradle_build() {
    info "Running Gradle build in $JAVA_DIR ..."
    cd "$JAVA_DIR"

    if [ ! -x ./gradlew ]; then
        chmod +x ./gradlew
    fi

    ./gradlew assembleDebug assembleRelease testJar "$@"

    info "Java build completed successfully."

    # Copy test JAR to location expected by desktop JVM tests
    local test_jar="$JAVA_DIR/build/libs/btleplug-jni.jar"
    if [ -f "$test_jar" ]; then
        local test_dest="$PROJECT_ROOT/target/debug/java/libs"
        mkdir -p "$test_dest"
        cp "$test_jar" "$test_dest/"
        info "Test JAR copied to $test_dest/"
    fi
}

# --- Main --------------------------------------------------------------------
main() {
    info "btleplug Java/Android build"
    echo

    ensure_java
    ensure_java_home
    ensure_android_sdk
    write_local_properties
    echo
    run_gradle_build "$@"
}

main "$@"
