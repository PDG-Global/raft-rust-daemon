#!/bin/bash
#
# Cross-Compilation Build Script for R-VPN
#
# Uses:
# - Native cargo for macOS targets (faster, avoids zig issues with some C libs)
# - cargo-zigbuild for Linux/FreeBSD targets (cross-compilation)
#

set -eo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Configuration
VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
DIST_DIR="./dist"
HOST_OS=$(uname -s)  # Darwin, Linux, FreeBSD

# macOS codesigning identity — leave empty for ad-hoc signing (development only)
# For distribution builds, use your Developer ID certificate hash:
#   CODESIGN_IDENTITY="4F896ADC521CD857E0777CC6D164F1FA13D812BD"
# Find valid identities with: security find-identity -v -p codesigning
CODESIGN_IDENTITY="${CODESIGN_IDENTITY:-4F896ADC521CD857E0777CC6D164F1FA13D812BD}"
# Notarization keychain profile (set to empty string to skip)
NOTARY_PROFILE="${NOTARY_PROFILE:-PDG}"

# Binaries to build
BINARIES=("rvpn" "rvpn-server")

# Targets
MACOS_TARGETS=("aarch64-apple-darwin" "x86_64-apple-darwin")
LINUX_GNU_TARGETS=("x86_64-unknown-linux-gnu" "aarch64-unknown-linux-gnu" "armv7-unknown-linux-gnueabihf" "riscv64gc-unknown-linux-gnu")
LINUX_MUSL_TARGETS=("x86_64-unknown-linux-musl" "aarch64-unknown-linux-musl")
# x86_64-unknown-freebsd cross-compiles reliably from any host via zigbuild.
# aarch64-unknown-freebsd is only built when running natively on a FreeBSD host
# (zig's aarch64-freebsd libc headers are under-tested, and boring/cmake cross-
# compilation from macOS/Linux to that target is not reliable).
FREEBSD_TARGETS=("x86_64-unknown-freebsd")
FREEBSD_NATIVE_TARGETS=("aarch64-unknown-freebsd")

ALL_TARGETS=("${MACOS_TARGETS[@]}" "${LINUX_GNU_TARGETS[@]}" "${LINUX_MUSL_TARGETS[@]}" "${FREEBSD_TARGETS[@]}" "${FREEBSD_NATIVE_TARGETS[@]}")

# Sign a macOS binary with Developer ID or ad-hoc identity
sign_macos() {
    local bin="$1"
    if [ -n "$CODESIGN_IDENTITY" ]; then
        codesign --force --sign "$CODESIGN_IDENTITY" --timestamp --options runtime "$bin" 2>/dev/null || true
    else
        codesign --force --sign - "$bin" 2>/dev/null || true
    fi
}

# Notarize a macOS binary via Apple's notary service
notarize_macos() {
    local bin="$1"
    if [ -z "$NOTARY_PROFILE" ]; then
        return 0
    fi
    
    local base
    base=$(basename "$bin")
    local dir
    dir=$(dirname "$bin")
    local zip_file="${dir}/${base}.zip"
    
    echo -n "  Notarizing ${base} ... "
    
    # Zip the binary for notarization
    zip -qj "$zip_file" "$bin" 2>/dev/null
    
    # Submit to Apple for notarization
    echo "(this may take 2-5 minutes)"
    local result
    result=$(xcrun notarytool submit "$zip_file" --keychain-profile "$NOTARY_PROFILE" --wait 2>&1) || true
    
    if echo "$result" | grep -q "status: Accepted"; then
        echo -e "${GREEN}✓${NC} Notarized"
        # Staple the ticket (ignore failures — binary still works notarized)
        xcrun stapler staple "$bin" 2>/dev/null || true
    else
        echo -e "${YELLOW}⚠ Notarization failed${NC}"
        echo "$result" | tail -5
    fi
    
    rm -f "$zip_file"
}

# Functions
print_header() {
    echo ""
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║                    R-VPN CROSS-COMPILATION BUILD                           ║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "Version: ${GREEN}${VERSION}${NC}"
    echo -e "Binaries: ${GREEN}${BINARIES[*]}${NC}"
    echo ""
}

check_prerequisites() {
    echo -e "${BLUE}ℹ Checking prerequisites...${NC}"

    # Check cargo
    if ! command -v cargo &> /dev/null; then
        echo -e "${RED}✗ cargo not found${NC}"
        exit 1
    fi
    echo -e "${GREEN}✓ cargo installed${NC}"

    # Check cargo-zigbuild (for Linux/FreeBSD)
    if ! command -v cargo-zigbuild &> /dev/null; then
        echo -e "${YELLOW}⚠ cargo-zigbuild not found (needed for Linux/FreeBSD builds)${NC}"
        echo -e "  Install with: ${CYAN}cargo install cargo-zigbuild${NC}"
        HAS_ZIGBUILD=false
    else
        echo -e "${GREEN}✓ cargo-zigbuild installed${NC}"
        HAS_ZIGBUILD=true
    fi

    # Check zig
    if ! command -v zig &> /dev/null; then
        echo -e "${YELLOW}⚠ zig not found (needed for Linux/FreeBSD builds)${NC}"
        if [ "$HOST_OS" = "FreeBSD" ]; then
            echo -e "  Install with: ${CYAN}pkg install zig${NC}"
        else
            echo -e "  Install with: ${CYAN}brew install zig${NC}  (macOS) or see ziglang.org"
        fi
    else
        echo -e "${GREEN}✓ zig installed${NC}"
    fi

    # Check lipo (for macOS universal binary)
    if ! command -v lipo &> /dev/null; then
        echo -e "${YELLOW}⚠ lipo not found (needed for macOS universal binary)${NC}"
    else
        echo -e "${GREEN}✓ lipo available${NC}"
    fi
}

build_binary() {
    local binary=$1
    local target=$2
    local use_zig=$3

    echo "  Building ${binary}..."

    local output_dir="target/${target}/release"

    # Ensure the Rust target is installed
    if ! rustup target list --installed | grep -q "^${target}$"; then
        echo -e "  ${YELLOW}Target ${target} not installed, adding...${NC}"
        rustup target add "$target"
    fi

    if [ "$use_zig" = true ]; then
        # Use cargo-zigbuild for cross-compilation
        if ! command -v cargo-zigbuild &> /dev/null; then
            echo -e "${RED}✗ cargo-zigbuild required for ${target}${NC}"
            return 1
        fi

        cargo zigbuild --release --bin "$binary" --target "$target" 2>&1 | while read line; do
            echo "    $line"
        done
    else
        # Use native cargo (for macOS targets on macOS host)
        cargo build --release --bin "$binary" --target "$target" 2>&1 | while read line; do
            echo "    $line"
        done
    fi

    local output_path="${output_dir}/${binary}"

    if [ -f "${output_path}" ]; then
        local size=$(ls -lh "${output_path}" | awk '{print $5}')
        echo -e "    ${GREEN}✓ ${binary} built${NC} (${size})"
        return 0
    else
        echo -e "    ${RED}✗ Binary not found at ${output_path}${NC}"
        return 1
    fi
}

build_target() {
    local target=$1
    local use_zig=$2

    echo ""
    echo -e "${BLUE}▶ Building for: ${CYAN}${target}${NC}"

    local failed=0

    for binary in "${BINARIES[@]}"; do
        if ! build_binary "$binary" "$target" "$use_zig"; then
            failed=1
        fi
    done

    return $failed
}

create_universal_binary() {
    local binary=$1

    echo ""
    echo -e "${BLUE}ℹ Creating macOS universal binary for ${binary}...${NC}"

    local arm64_bin="target/aarch64-apple-darwin/release/${binary}"
    local x86_64_bin="target/x86_64-apple-darwin/release/${binary}"
    local universal_bin="${DIST_DIR}/${binary}-macos-universal"

    if [ ! -f "$arm64_bin" ]; then
        echo -e "${YELLOW}⚠ ARM64 binary not found, skipping universal binary${NC}"
        return 1
    fi

    if [ ! -f "$x86_64_bin" ]; then
        echo -e "${YELLOW}⚠ x86_64 binary not found, skipping universal binary${NC}"
        return 1
    fi

    mkdir -p "${DIST_DIR}"
    lipo -create "$arm64_bin" "$x86_64_bin" -output "$universal_bin"

    # Re-sign universal binary after lipo to fix Gatekeeper issues
    sign_macos "$universal_bin"

    local size=$(ls -lh "$universal_bin" | awk '{print $5}')
    echo -e "${GREEN}✓ Universal binary created${NC} (${size})"
    echo "  Location: ${universal_bin}"

    # Verify
    echo "  Architectures: $(lipo -archs "$universal_bin")"
}

copy_artifacts() {
    echo ""
    echo -e "${BLUE}ℹ Copying build artifacts...${NC}"

    mkdir -p "${DIST_DIR}"

    # Determine which platform families were built
    local have_macos=false
    local have_linux_gnu=false
    local have_linux_musl=false
    local have_freebsd=false
    for target in "${targets_to_build[@]}"; do
        case "$target" in
            *-apple-darwin) have_macos=true ;;
            *-linux-gnu*|*-linux-gnueabihf) have_linux_gnu=true ;;
            *-linux-musl) have_linux_musl=true ;;
            *-freebsd) have_freebsd=true ;;
        esac
    done

    for binary in "${BINARIES[@]}"; do
        # macOS native binaries (only if macOS targets were built)
        if [ "$have_macos" = true ]; then
            if [ -f "target/aarch64-apple-darwin/release/${binary}" ]; then
                cp "target/aarch64-apple-darwin/release/${binary}" "${DIST_DIR}/${binary}-macos-arm64"
                sign_macos "${DIST_DIR}/${binary}-macos-arm64"
                notarize_macos "${DIST_DIR}/${binary}-macos-arm64"
                echo -e "${GREEN}✓${NC} ${binary}-macos-arm64"
            fi

            if [ -f "target/x86_64-apple-darwin/release/${binary}" ]; then
                cp "target/x86_64-apple-darwin/release/${binary}" "${DIST_DIR}/${binary}-macos-x86_64"
                sign_macos "${DIST_DIR}/${binary}-macos-x86_64"
                notarize_macos "${DIST_DIR}/${binary}-macos-x86_64"
                echo -e "${GREEN}✓${NC} ${binary}-macos-x86_64"
            fi
        fi

        # Linux GNU binaries
        if [ "$have_linux_gnu" = true ]; then
            for target in "${LINUX_GNU_TARGETS[@]}"; do
                if [ -f "target/${target}/release/${binary}" ]; then
                    local name=$(echo "$target" | sed 's/-unknown//g' | sed 's/-gnueabihf//g')
                    cp "target/${target}/release/${binary}" "${DIST_DIR}/${binary}-${name}"
                    echo -e "${GREEN}✓${NC} ${binary}-${name}"
                fi
            done
        fi

        # Linux MUSL binaries (static)
        if [ "$have_linux_musl" = true ]; then
            for target in "${LINUX_MUSL_TARGETS[@]}"; do
                if [ -f "target/${target}/release/${binary}" ]; then
                    local name=$(echo "$target" | sed 's/-unknown//g' | sed 's/-musl//g')
                    cp "target/${target}/release/${binary}" "${DIST_DIR}/${binary}-${name}-static"
                    echo -e "${GREEN}✓${NC} ${binary}-${name}-static"
                fi
            done
        fi

        # FreeBSD binaries
        if [ "$have_freebsd" = true ]; then
            for target in "${FREEBSD_TARGETS[@]}"; do
                if [ -f "target/${target}/release/${binary}" ]; then
                    local name=$(echo "$target" | sed 's/-unknown//g')
                    cp "target/${target}/release/${binary}" "${DIST_DIR}/${binary}-${name}"
                    echo -e "${GREEN}✓${NC} ${binary}-${name}"
                fi
            done
        fi
    done

    # Android APK (built separately via rvpn-android/build_rust.sh + gradle assembleRelease)
    local android_apk="../rvpn-android/app/build/outputs/apk/release/app-release.apk"
    if [ -f "$android_apk" ]; then
        cp "$android_apk" "${DIST_DIR}/rvpn-android.apk"
        echo -e "${GREEN}✓${NC} rvpn-android.apk"
    else
        echo -e "${YELLOW}⚠ Android APK not found at ${android_apk} — run build_rust.sh + gradle assembleRelease in rvpn-android/${NC}"
    fi

    # Create and notarize universal macOS binaries (only if macOS targets were built)
    if [ "$have_macos" = true ]; then
        for binary in "${BINARIES[@]}"; do
            if [ -f "target/aarch64-apple-darwin/release/${binary}" ] && \
               [ -f "target/x86_64-apple-darwin/release/${binary}" ]; then
                create_universal_binary "$binary"
                if [ -f "${DIST_DIR}/${binary}-macos-universal" ]; then
                    sign_macos "${DIST_DIR}/${binary}-macos-universal"
                    notarize_macos "${DIST_DIR}/${binary}-macos-universal"
                    echo -e "${GREEN}✓${NC} ${binary}-macos-universal"
                fi
            fi
        done
    fi
}

generate_checksums() {
    echo ""
    echo -e "${BLUE}ℹ Creating checksums...${NC}"

    # Generate individual .sha256 files for each binary
    for binary in "${DIST_DIR}"/rvpn-*; do
        if [ -f "$binary" ] && [[ ! "$binary" == *.sha256 ]]; then
            local basename=$(basename "$binary")
            if command -v sha256sum &> /dev/null; then
                sha256sum "$binary" | awk '{print $1}' > "${binary}.sha256"
            else
                shasum -a 256 "$binary" | awk '{print $1}' > "${binary}.sha256"
            fi
            echo -e "  ${GREEN}✓${NC} ${basename}.sha256"
        fi
    done

    # Also generate a combined SHA256SUMS.txt file
    if command -v sha256sum &> /dev/null; then
        (cd "${DIST_DIR}" && sha256sum rvpn-* > SHA256SUMS.txt)
    else
        (cd "${DIST_DIR}" && shasum -a 256 rvpn-* > SHA256SUMS.txt)
    fi

    echo -e "${GREEN}✓ Checksums saved to individual .sha256 files and SHA256SUMS.txt${NC}"
}

print_summary() {
    echo ""
    echo -e "${CYAN}════════════════════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}✓ Build complete!${NC}"
    echo ""
    echo -e "Artifacts in ${CYAN}${DIST_DIR}/${NC}:"
    ls -lh "${DIST_DIR}"/rvpn-* 2>/dev/null | awk '{print "  " $9 " (" $5 ")"}'
    echo ""
    echo -e "${CYAN}════════════════════════════════════════════════════════════════════════════════${NC}"
}

# Main
main() {
    print_header
    check_prerequisites

    local targets_to_build=()
    local failed_targets=()
    local build_macos=false
    local build_linux=false
    local build_musl=false
    local build_freebsd=false

    # Parse arguments
    if [ $# -eq 0 ]; then
        if [ "$HOST_OS" = "FreeBSD" ]; then
            # On FreeBSD: build Linux and FreeBSD targets (skip macOS — no lipo/codesign)
            # aarch64-unknown-freebsd is only built here; cross-compiling it from
            # macOS/Linux via zigbuild is not reliable.
            echo -e "${BLUE}ℹ FreeBSD host detected — building Linux + FreeBSD targets (incl. aarch64)${NC}"
            build_linux=true
            build_musl=true
            build_freebsd=true
            targets_to_build+=("${FREEBSD_NATIVE_TARGETS[@]}")
        else
            # Default: build everything
            build_macos=true
            build_linux=true
            build_musl=true
            build_freebsd=true
        fi
    else
        for arg in "$@"; do
            case "$arg" in
                --macos-only)
                    build_macos=true
                    ;;
                --linux-only)
                    build_linux=true
                    ;;
                --musl-only)
                    build_musl=true
                    ;;
                --freebsd-only)
                    build_freebsd=true
                    ;;
                aarch64-apple-darwin|x86_64-apple-darwin|x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|armv7-unknown-linux-gnueabihf|x86_64-unknown-linux-musl|aarch64-unknown-linux-musl|x86_64-unknown-freebsd|aarch64-unknown-freebsd)
                    targets_to_build+=("$arg")
                    ;;
                *)
                    echo -e "${RED}Unknown target or option: $arg${NC}"
                    echo "Usage: $0 [--macos-only|--linux-only|--musl-only|--freebsd-only|<target-triple>]"
                    exit 1
                    ;;
            esac
        done
    fi

    # Determine which targets to build
    if [ ${#targets_to_build[@]} -eq 0 ]; then
        if [ "$build_macos" = true ]; then
            targets_to_build+=("${MACOS_TARGETS[@]}")
        fi
        if [ "$build_linux" = true ]; then
            targets_to_build+=("${LINUX_GNU_TARGETS[@]}")
        fi
        if [ "$build_musl" = true ]; then
            targets_to_build+=("${LINUX_MUSL_TARGETS[@]}")
        fi
        if [ "$build_freebsd" = true ]; then
            targets_to_build+=("${FREEBSD_TARGETS[@]}")
        fi
    fi

    echo -e "${BLUE}ℹ Building ${#targets_to_build[@]} target(s)...${NC}"

    # Build each target
    for target in "${targets_to_build[@]}"; do
        # Determine if we should use zig for this target
        local use_zig=false
        case "$target" in
            *-linux-*|*-freebsd)
                use_zig=true
                ;;
        esac

        if ! build_target "$target" "$use_zig"; then
            failed_targets+=("$target")
        fi
    done

    # Copy artifacts (includes creating & notarizing universal macOS binaries)
    copy_artifacts

    # Generate checksums
    generate_checksums

    # Print summary
    print_summary

    # Report failures
    if [ ${#failed_targets[@]} -gt 0 ]; then
        echo ""
        echo -e "${RED}✗ Some builds failed:${NC}"
        for target in "${failed_targets[@]}"; do
            echo "  - $target"
        done
        exit 1
    fi
}

main "$@"
