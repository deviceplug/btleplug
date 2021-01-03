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
        rm --force "${this_dir}/bluez-dbus-introspect-manager.xml.tmp"
        rm --force "${this_dir}/bluez-dbus-introspect-adapter.xml.tmp"
        rm --force "${this_dir}/bluez-dbus-introspect-device.xml.tmp"
        rm --force "${this_dir}/bluez-dbus-introspect-gatt-service.xml.tmp"
        rm --force "${this_dir}/bluez-dbus-introspect-gatt-characteristic.xml.tmp"
        rm --force "${this_dir}/bluez-dbus-introspect-gatt-descriptor.xml.tmp"
    fi
}

main () {
    trap cleanup EXIT

    echo "> Checking required programs."

    if hash dbus-send 2>/dev/null; then
        echo "> Command 'dbus-send' is present."
    else
        echo "> Command 'dbus-send' is absent."
        echo "> This command is usually found in the same package as DBus."
        echo "> In Ubuntu, this is can be installed with 'sudo apt install dbus'"
        exit 1
    fi
    if hash xmllint 2>/dev/null; then
        echo "> Command 'xmllint' is present."
    else
        echo "> Command 'xmllint' is absent."
        echo "> This command is usually found in the libxml2 utils package."
        echo "> In Ubuntu, this is can be installed with 'sudo apt install libxml2-utils'"
        exit 1
    fi
    echo "> All required programs are present."

    echo "> Starting introspection"

    echo -n "> Introspecting manager"

    introspect "/org/bluez" "${this_dir}/bluez-dbus-introspect-manager.xml.tmp"
    adapter_names="$(node_names "${this_dir}/bluez-dbus-introspect-manager.xml.tmp")"
    output_introspection "${this_dir}/bluez-dbus-introspect-manager.xml.tmp" "${this_dir}/bluez-dbus-introspect-manager.xml" "false"

    echo "> Found BlueZ manager types. Now introspecting all $(lines "${adapter_names}") discovered adapter."
    local has_adapters=false has_devices=false has_services=false has_characteristic=false has_descriptor=false

    for adapter_name in ${adapter_names}; do
        echo -n ">> Introspecting adapter ${adapter_name}."
        introspect "/org/bluez/${adapter_name}" "${this_dir}/bluez-dbus-introspect-adapter.xml.tmp"
        device_names="$(node_names "${this_dir}/bluez-dbus-introspect-adapter.xml.tmp")"
        output_introspection "${this_dir}/bluez-dbus-introspect-adapter.xml.tmp" "${this_dir}/bluez-dbus-introspect-adapter.xml" "${has_adapters}"
        has_adapters=true

        echo ">> Found BlueZ adapter types. Now introspecting all $(lines "${device_names}") discovered devices."

        for device_name in ${device_names}; do
            echo -n ">>> Introspecting device ${adapter_name}/${device_name}."
            introspect "/org/bluez/${adapter_name}/${device_name}" "${this_dir}/bluez-dbus-introspect-device.xml.tmp"
            service_names="$(node_names "${this_dir}/bluez-dbus-introspect-device.xml.tmp")"
            output_introspection "${this_dir}/bluez-dbus-introspect-device.xml.tmp" "${this_dir}/bluez-dbus-introspect-device.xml" "${has_devices}"
            has_devices=true

            echo ">> Found BlueZ device types. Now introspecting all $(lines "${service_names}") discovered services."

            for service_name in ${service_names}; do
                echo -n ">>> Introspecting service ${adapter_name}/${device_name}/${service_name}."
                introspect "/org/bluez/${adapter_name}/${device_name}/${service_name}" "${this_dir}/bluez-dbus-introspect-gatt-service.xml.tmp"
                characteristic_names="$(node_names "${this_dir}/bluez-dbus-introspect-gatt-service.xml.tmp")"
                output_introspection "${this_dir}/bluez-dbus-introspect-gatt-service.xml.tmp" "${this_dir}/bluez-dbus-introspect-gatt-service.xml" "${has_services}"
                has_services=true

                echo ">> Found BlueZ service types. Now introspecting all $(lines "${characteristic_names}") discovered characteristics."

                for characteristic_name in ${characteristic_names}; do
                    echo -n ">>> Introspecting characteristic ${adapter_name}/${device_name}/${service_name}/${characteristic_name}."
                    introspect "/org/bluez/${adapter_name}/${device_name}/${service_name}/${characteristic_name}" "${this_dir}/bluez-dbus-introspect-gatt-characteristic.xml.tmp"
                    descriptor_names="$(node_names "${this_dir}/bluez-dbus-introspect-gatt-characteristic.xml.tmp")"
                    output_introspection "${this_dir}/bluez-dbus-introspect-gatt-characteristic.xml.tmp" "${this_dir}/bluez-dbus-introspect-gatt-characteristic.xml" "${has_characteristic}"
                    has_characteristic=true

                    echo ">> Found BlueZ characteristic types. Now introspecting all $(lines "${descriptor_names}") discovered descriptors."

                    for descriptor_name in ${descriptor_names}; do
                        echo -n ">>> Introspecting descriptor ${adapter_name}/${device_name}/${service_name}/${characteristic_name}/${descriptor_name}."
                        introspect "/org/bluez/${adapter_name}/${device_name}/${service_name}/${characteristic_name}/${descriptor_name}" "${this_dir}/bluez-dbus-introspect-gatt-descriptor.xml.tmp"
                        output_introspection "${this_dir}/bluez-dbus-introspect-gatt-descriptor.xml.tmp" "${this_dir}/bluez-dbus-introspect-gatt-descriptor.xml" "${has_descriptor}"
                        has_descriptor=true
                    done
                done
            done
        done
    done

    echo "Finished introspection"
    echo

    if [[ "${has_adapters}" = "false" ]]; then
        echo "Unable to find an adapter to export, leaving the adapter and device XML files alone."
        echo
        echo "Updated 'bluez-dbus-introspect-manager.xml'"
    elif [[ "${has_devices}" = "false" ]]; then
        echo "Unable to find a device to export, leaving the device XML file alone"
        echo "To fix this you should connectet to a device before running this script."
        echo
        echo "Updated 'bluez-dbus-introspect-manager.xml', 'bluez-dbus-introspect-adapter.xml'"
    elif [[ "${has_services}" = "false" ]]; then
        echo "Unable to find a device with services to export, leaving the service XML file alone"
        echo "To fix this you should connectet to a device before running this script."
        echo
        echo "Updated 'bluez-dbus-introspect-manager.xml', 'bluez-dbus-introspect-adapter.xml', 'bluez-dbus-introspect-device.xml'"
    elif [[ "${has_characteristic}" = "false" ]]; then
        echo "Unable to find a device with characteristics to export, leaving the characteristics XML file alone"
        echo "To fix this you should connectet to a device with at least one characteristic before running this script."
        echo
        echo "Updated 'bluez-dbus-introspect-manager.xml', 'bluez-dbus-introspect-adapter.xml', 'bluez-dbus-introspect-device.xml', 'bluez-dbus-introspect-gatt-service.xml'"
    elif [[ "${has_descriptor}" = "false" ]]; then
        echo "Unable to find a device with descriptor to export, leaving the descriptor XML file alone"
        echo "To fix this you should connectet to a device with at least one characteristic with a descriptor before running this script."
        echo
        echo "Updated 'bluez-dbus-introspect-manager.xml', 'bluez-dbus-introspect-adapter.xml', 'bluez-dbus-introspect-device.xml', 'bluez-dbus-introspect-gatt-service.xml', 'bluez-dbus-introspect-gatt-characteristic.xml'"
    else
        echo "Updated 'bluez-dbus-introspect-manager.xml', 'bluez-dbus-introspect-adapter.xml', 'bluez-dbus-introspect-device.xml', 'bluez-dbus-introspect-gatt-service.xml', 'bluez-dbus-introspect-gatt-characteristic.xml', 'bluez-dbus-introspect-gatt-descriptor.xml'"
    fi
}
main "$@"
