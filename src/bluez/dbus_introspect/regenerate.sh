#!/bin/bash

set -euo pipefail
[[ -z "${DEBUG+x}" ]] || set -ex
IFS=$'\n\t'

readonly this_script="$(readlink -f "$0")"
readonly this_dir="$(dirname "$this_script")"

readonly NODE_NAME_REGEX='^ +<node name="([^"]+)"\/>$'

introspect() {
    local -r dbus_path="$1"
    local -r output_path="$2"
    dbus-send --system \
              --dest=org.bluez \
              --type=method_call \
              --print-reply=literal \
              --reply-timeout=5000 \
              "${dbus_path}" \
              org.freedesktop.DBus.Introspectable.Introspect \
        | xmllint --format - > "${output_path}"
}

lines() {
    local -r input="$1"
    if [[ -z "${input}" ]]; then
        echo "0"
    else
        echo "${input}" | wc -l
    fi
}

node_names() {
    local -r input_file="$1"

    sed -nE 's/'"${NODE_NAME_REGEX}"'/\1/gp' "${input_file}"
}

output_introspection() {
    local -r input_file="$1"
    local -r output_file="$2"
    local -r is_subsequent_output="$3"

    if [[ "${is_subsequent_output}" = "true" || -n "${CHECK+x}" ]]; then
        # Check the 2nd and subsequent outputs against the first one.
        # Or if the `CHECK` variable is set.
        if diff "${output_file}" <(sed -E '/'"${NODE_NAME_REGEX}"'/d' "${input_file}"); then
            echo " (output matches)"
        else
            echo " (output doesn't match)"
        fi
    else
        sed -E '/'"${NODE_NAME_REGEX}"'/d' "${input_file}" > "${output_file}"
        echo " (output)"
    fi
}



cleanup() {
    if [[ -z "${DEBUG+x}" ]]; then
        # only remove when `DEBUG` is unset.
        rm --force "${this_dir}/dbus-introspect-manager.xml.tmp"
        rm --force "${this_dir}/dbus-introspect-adapter.xml.tmp"
        rm --force "${this_dir}/dbus-introspect-device.xml.tmp"
    fi
}

main () {
    trap cleanup EXIT


    echo "> Starting introspection"

    echo -n "> Introspecting manager"

    introspect "/org/bluez" "${this_dir}/dbus-introspect-manager.xml.tmp"
    adapter_names="$(node_names "${this_dir}/dbus-introspect-manager.xml.tmp")"
    output_introspection "${this_dir}/dbus-introspect-manager.xml.tmp" "${this_dir}/dbus-introspect-manager.xml" "false"

    echo "> Found BlueZ manager types. Now introspecting all $(lines "${adapter_names}") discovered adapter."
    local has_adapters=false has_devices=false

    for adapter_name in ${adapter_names}; do
        echo -n ">> Introspecting adapter ${adapter_name}."
        introspect "/org/bluez/${adapter_name}" "${this_dir}/dbus-introspect-adapter.xml.tmp"
        device_names="$(node_names "${this_dir}/dbus-introspect-adapter.xml.tmp")"
        output_introspection "${this_dir}/dbus-introspect-adapter.xml.tmp" "${this_dir}/dbus-introspect-adapter.xml" "${has_adapters}"
        has_adapters=true

        echo ">> Found BlueZ adapter types. Now introspecting all $(lines "${device_names}") discovered devices."

        for device_name in ${device_names}; do
            echo -n ">>> Introspecting device ${adapter_name}/${device_name}."
            introspect "/org/bluez/${adapter_name}/${device_name}" "${this_dir}/dbus-introspect-device.xml.tmp"
            output_introspection "${this_dir}/dbus-introspect-device.xml.tmp" "${this_dir}/dbus-introspect-device.xml" "${has_devices}"
            has_devices=true
        done
    done

    echo "Finished introspection"
    echo

    if [[ "${has_adapters}" = "false" ]]; then
        echo "Unable to find an adapter to export, leaving the adapter and device XML files alone."
        echo
        echo "Updated 'dbus-introspect-manager.xml'"
    elif [[ "${has_devices}" = "false" ]]; then
        echo "Unable to find a device to export, leaving the device XML file alone"
        echo "To fix this you should start a scan before running this script."
        echo
        echo "Updated 'dbus-introspect-manager.xml', 'dbus-introspect-adapter.xml'"
    else
        echo "Updated 'dbus-introspect-manager.xml', 'dbus-introspect-adapter.xml', 'dbus-introspect-device.xml'"
    fi
}
main "$@"
