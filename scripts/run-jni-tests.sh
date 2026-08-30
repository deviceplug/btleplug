#!/usr/bin/env bash
#
# Compile gedgygedgy Java sources with plain javac (no Android SDK) and run
# the jni_utils Rust tests on the host JVM.
#
# Usage:  scripts/run-jni-tests.sh
#
# Environment:
#   JAVA_HOME  — override JDK location (auto-detected if unset)
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
JAVA_SRC_DIR="$PROJECT_ROOT/src/droidplug/java/src/main/java"
BUILD_DIR="$PROJECT_ROOT/target/debug/java"
JAR_DIR="$BUILD_DIR/libs"
JAR_PATH="$JAR_DIR/btleplug-jni.jar"

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

# --- Java (reuses patterns from build-java.sh) -------------------------------
ensure_java() {
    if command -v javac &>/dev/null && javac -version &>/dev/null 2>&1; then
        local ver
        ver="$(javac -version 2>&1 | head -1 | sed -E 's/javac ([0-9]+).*/\1/')"
        if [ "$ver" -ge 11 ] 2>/dev/null; then
            info "javac $ver found: $(command -v javac)"
            return 0
        else
            warn "javac $ver found but JNI tests require Java >= 11"
        fi
    fi

    die "javac not found or too old. Install a JDK >= 11 (e.g. openjdk-17) and re-run."
}

ensure_java_home() {
    if [ -n "${JAVA_HOME:-}" ] && [ -d "$JAVA_HOME" ]; then
        info "JAVA_HOME=$JAVA_HOME"
        return 0
    fi

    case "$OS" in
        Darwin)
            JAVA_HOME="$(/usr/libexec/java_home 2>/dev/null || true)"
            if [ -z "${JAVA_HOME:-}" ] || [ ! -d "${JAVA_HOME:-}" ]; then
                for v in 17 21 11; do
                    local brew_jdk
                    brew_jdk="$(brew --prefix "openjdk@$v" 2>/dev/null || true)"
                    if [ -n "$brew_jdk" ] && [ -d "$brew_jdk/libexec/openjdk.jdk/Contents/Home" ]; then
                        JAVA_HOME="$brew_jdk/libexec/openjdk.jdk/Contents/Home"
                        break
                    fi
                done
            fi
            ;;
        Linux)
            for candidate in \
                /usr/lib/jvm/java-17-openjdk-amd64 \
                /usr/lib/jvm/java-17-openjdk \
                /usr/lib/jvm/java-17 \
                /usr/lib/jvm/default-java; do
                if [ -d "$candidate" ]; then
                    JAVA_HOME="$candidate"
                    break
                fi
            done
            ;;
    esac

    if [ -z "${JAVA_HOME:-}" ]; then
        die "Could not determine JAVA_HOME. Set it manually and re-run."
    fi
    export JAVA_HOME
    info "JAVA_HOME=$JAVA_HOME"
}

# --- Compile Java sources ----------------------------------------------------
compile_java() {
    info "Compiling gedgygedgy Java sources..."

    local classes_dir="$BUILD_DIR/classes"
    rm -rf "$classes_dir"
    mkdir -p "$classes_dir"

    # Only compile the plain-Java rust support classes — the nonpolynomial
    # sources depend on Android APIs and cannot be compiled with plain javac.
    local rust_dir="$JAVA_SRC_DIR/io/github/gedgygedgy/rust"
    local sources=()
    while IFS= read -r -d '' f; do
        sources+=("$f")
    done < <(find "$rust_dir" -name '*.java' -print0)

    if [ ${#sources[@]} -eq 0 ]; then
        die "No .java files found under $rust_dir"
    fi

    info "Found ${#sources[@]} Java source files"

    javac -d "$classes_dir" --release 11 -sourcepath "$JAVA_SRC_DIR" "${sources[@]}"

    info "Compilation successful"
}

# --- Package JAR -------------------------------------------------------------
package_jar() {
    info "Packaging btleplug-jni.jar..."

    mkdir -p "$JAR_DIR"

    jar cf "$JAR_PATH" -C "$BUILD_DIR/classes" .

    info "JAR created at $JAR_PATH"
}

# --- Run Cargo tests ---------------------------------------------------------
run_tests() {
    info "Running jni_utils tests..."

    cd "$PROJECT_ROOT"
    cargo test --features jni-host-tests -- --test-threads=1

    info "All tests passed!"
}

# --- Main --------------------------------------------------------------------
main() {
    info "btleplug JNI host tests"
    echo

    ensure_java
    ensure_java_home
    echo

    compile_java
    package_jar
    echo

    run_tests
}

main "$@"
