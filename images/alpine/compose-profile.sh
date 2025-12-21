#!/bin/bash
# compose-profile.sh - Compose a profile from feature overlays
#
# This script reads a profile definition and composes all feature
# overlays into a unified configuration for image building.
#
# Usage: compose-profile.sh <profile.yaml> [--output-dir DIR] [--validate-only]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FEATURES_DIR="${SCRIPT_DIR}/features"
PROFILES_DIR="${SCRIPT_DIR}/profiles"
SCHEMAS_DIR="${SCRIPT_DIR}/schemas"
OUTPUT_DIR="${OUTPUT_DIR:-${SCRIPT_DIR}/build}"

log() {
    echo "[$(date -Iseconds)] $*"
}

error() {
    echo "[$(date -Iseconds)] ERROR: $*" >&2
    exit 1
}

# Parse arguments
PROFILE_FILE=""
VALIDATE_ONLY=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --validate-only)
            VALIDATE_ONLY=true
            shift
            ;;
        *)
            PROFILE_FILE="$1"
            shift
            ;;
    esac
done

if [[ -z "$PROFILE_FILE" ]]; then
    echo "Usage: $0 <profile.yaml> [--output-dir DIR] [--validate-only]"
    echo ""
    echo "Available profiles:"
    ls -1 "$PROFILES_DIR"/*.yaml 2>/dev/null | xargs -n1 basename | sed 's/.yaml$//'
    exit 1
fi

# Find profile file
if [[ ! -f "$PROFILE_FILE" ]]; then
    if [[ -f "${PROFILES_DIR}/${PROFILE_FILE}" ]]; then
        PROFILE_FILE="${PROFILES_DIR}/${PROFILE_FILE}"
    elif [[ -f "${PROFILES_DIR}/${PROFILE_FILE}.yaml" ]]; then
        PROFILE_FILE="${PROFILES_DIR}/${PROFILE_FILE}.yaml"
    else
        error "Profile not found: $PROFILE_FILE"
    fi
fi

log "Composing profile: $PROFILE_FILE"

# Check for required tools
for cmd in yq jq; do
    if ! command -v "$cmd" &> /dev/null; then
        error "Required tool not found: $cmd"
    fi
done

# Validate profile schema
validate_profile() {
    local profile="$1"
    
    # Basic validation - check required fields
    local name base
    name=$(yq -r '.name // empty' "$profile")
    base=$(yq -r '.base // empty' "$profile")
    
    if [[ -z "$name" ]]; then
        error "Profile missing 'name' field"
    fi
    
    if [[ -z "$base" ]]; then
        error "Profile missing 'base' field"
    fi
    
    log "Profile: $name (base: $base)"
}

# Load feature definition
load_feature() {
    local feature_name="$1"
    local feature_dir="${FEATURES_DIR}/${feature_name}"
    
    if [[ ! -d "$feature_dir" ]]; then
        error "Feature not found: $feature_name"
    fi
    
    if [[ ! -f "${feature_dir}/feature.yaml" ]]; then
        error "Feature missing feature.yaml: $feature_name"
    fi
    
    echo "$feature_dir"
}

# Check feature dependencies and conflicts
check_dependencies() {
    local feature_name="$1"
    local feature_file="${FEATURES_DIR}/${feature_name}/feature.yaml"
    local features_list="$2"
    
    # Check requires
    local requires
    requires=$(yq -r '.requires[]? // empty' "$feature_file")
    
    for req in $requires; do
        if ! echo "$features_list" | grep -q "^${req}$"; then
            error "Feature '$feature_name' requires '$req' which is not in the profile"
        fi
    done
    
    # Check conflicts
    local conflicts
    conflicts=$(yq -r '.conflicts[]? // empty' "$feature_file")
    
    for conflict in $conflicts; do
        if echo "$features_list" | grep -q "^${conflict}$"; then
            # This is a warning, not an error - profiles can override
            log "WARNING: Feature '$feature_name' conflicts with '$conflict'"
        fi
    done
}

# Collect packages from all features
collect_packages() {
    local packages=""
    
    while IFS= read -r feature; do
        if [[ -z "$feature" ]]; then
            continue
        fi
        
        local feature_file="${FEATURES_DIR}/${feature}/feature.yaml"
        if [[ -f "$feature_file" ]]; then
            local pkgs
            pkgs=$(yq -r '.packages[]? // empty' "$feature_file")
            packages="${packages}${pkgs}"$'\n'
        fi
    done
    
    echo "$packages" | sort -u | grep -v '^$' || true
}

# Collect services to enable
collect_services_enable() {
    local services=""
    
    while IFS= read -r feature; do
        if [[ -z "$feature" ]]; then
            continue
        fi
        
        local feature_file="${FEATURES_DIR}/${feature}/feature.yaml"
        if [[ -f "$feature_file" ]]; then
            local svcs
            svcs=$(yq -r '.services_enable[]? // empty' "$feature_file")
            services="${services}${svcs}"$'\n'
        fi
    done
    
    echo "$services" | sort -u | grep -v '^$' || true
}

# Collect services to disable
collect_services_disable() {
    local services=""
    
    while IFS= read -r feature; do
        if [[ -z "$feature" ]]; then
            continue
        fi
        
        local feature_file="${FEATURES_DIR}/${feature}/feature.yaml"
        if [[ -f "$feature_file" ]]; then
            local svcs
            svcs=$(yq -r '.services_disable[]? // empty' "$feature_file")
            services="${services}${svcs}"$'\n'
        fi
    done
    
    echo "$services" | sort -u | grep -v '^$' || true
}

# Collect files from all features
collect_files() {
    while IFS= read -r feature; do
        if [[ -z "$feature" ]]; then
            continue
        fi
        
        local feature_dir="${FEATURES_DIR}/${feature}"
        local feature_file="${feature_dir}/feature.yaml"
        
        if [[ -f "$feature_file" ]]; then
            local files_count
            files_count=$(yq -r '.files | length' "$feature_file" 2>/dev/null || echo "0")
            
            for ((i=0; i<files_count; i++)); do
                local source dest mode
                source=$(yq -r ".files[$i].source" "$feature_file")
                dest=$(yq -r ".files[$i].destination" "$feature_file")
                mode=$(yq -r ".files[$i].mode // \"0644\"" "$feature_file")
                
                echo "${feature_dir}/${source}|${dest}|${mode}"
            done
        fi
    done
}

# Collect firewall fragments
collect_firewall_fragments() {
    while IFS= read -r feature; do
        if [[ -z "$feature" ]]; then
            continue
        fi
        
        local feature_file="${FEATURES_DIR}/${feature}/feature.yaml"
        
        if [[ -f "$feature_file" ]]; then
            local fragment
            fragment=$(yq -r '.firewall_fragment // empty' "$feature_file")
            
            if [[ -n "$fragment" ]]; then
                echo "# Feature: $feature"
                echo "$fragment"
                echo ""
            fi
        fi
    done
}

# Collect selftest modules
collect_selftests() {
    while IFS= read -r feature; do
        if [[ -z "$feature" ]]; then
            continue
        fi
        
        local feature_dir="${FEATURES_DIR}/${feature}"
        local selftest_dir="${feature_dir}/selftest"
        
        if [[ -d "$selftest_dir" ]]; then
            for test_file in "$selftest_dir"/*.py; do
                if [[ -f "$test_file" ]]; then
                    echo "${test_file}|/usr/share/infrasim/selftest/$(basename "$test_file")"
                fi
            done
        fi
    done
}

# Main composition logic
compose() {
    local profile="$1"
    
    validate_profile "$profile"
    
    local profile_name
    profile_name=$(yq -r '.name' "$profile")
    
    local profile_output_dir="${OUTPUT_DIR}/${profile_name}"
    mkdir -p "$profile_output_dir"
    
    # Get feature list
    local features
    features=$(yq -r '.features[]? // empty' "$profile")
    
    if [[ -z "$features" ]]; then
        error "Profile has no features defined"
    fi
    
    log "Features: $(echo "$features" | tr '\n' ' ')"
    
    # Check dependencies for each feature
    while IFS= read -r feature; do
        if [[ -z "$feature" ]]; then
            continue
        fi
        check_dependencies "$feature" "$features"
    done <<< "$features"
    
    if [[ "$VALIDATE_ONLY" == "true" ]]; then
        log "Validation passed"
        return 0
    fi
    
    # Collect all components
    log "Collecting packages..."
    local packages
    packages=$(echo "$features" | collect_packages)
    echo "$packages" > "${profile_output_dir}/packages.txt"
    log "  $(echo "$packages" | wc -l | tr -d ' ') packages"
    
    log "Collecting services..."
    local services_enable services_disable
    services_enable=$(echo "$features" | collect_services_enable)
    services_disable=$(echo "$features" | collect_services_disable)
    echo "$services_enable" > "${profile_output_dir}/services-enable.txt"
    echo "$services_disable" > "${profile_output_dir}/services-disable.txt"
    
    log "Collecting files..."
    local files
    files=$(echo "$features" | collect_files)
    echo "$files" > "${profile_output_dir}/files.txt"
    log "  $(echo "$files" | grep -c '|' || echo 0) files"
    
    log "Collecting firewall fragments..."
    local firewall
    firewall=$(echo "$features" | collect_firewall_fragments)
    echo "$firewall" > "${profile_output_dir}/firewall.nft"
    
    log "Collecting selftests..."
    local selftests
    selftests=$(echo "$features" | collect_selftests)
    echo "$selftests" > "${profile_output_dir}/selftests.txt"
    
    # Copy feature configs
    log "Merging feature configs..."
    local feature_config
    feature_config=$(yq -r '.feature_config // {}' "$profile")
    echo "$feature_config" | yq -y '.' > "${profile_output_dir}/feature-config.yaml"
    
    # Copy profile metadata
    cp "$profile" "${profile_output_dir}/profile.yaml"
    
    # Generate manifest
    log "Generating manifest..."
    cat > "${profile_output_dir}/manifest.json" <<EOF
{
  "profile": "$profile_name",
  "version": "$(yq -r '.version // "1.0.0"' "$profile")",
  "base": "$(yq -r '.base' "$profile")",
  "features": $(yq -j '.features // []' "$profile"),
  "generated_at": "$(date -Iseconds)",
  "git_sha": "$(git rev-parse HEAD 2>/dev/null || echo 'unknown')"
}
EOF
    
    log "Profile composed to: $profile_output_dir"
    log ""
    log "Contents:"
    ls -la "$profile_output_dir"
}

compose "$PROFILE_FILE"
