#!/bin/bash
type mosquitto_sub > /dev/null && type mosquitto_pub > /dev/null
if [ $? -ne 0 ]; then
	echo "This script requires 'mosquitto_sub' and 'mosquitto_pub'."
	echo "These tools are not found. To install those on Ubuntu"
	echo "or alike, run:"
	echo ""
	echo "	sudo apt install mosquitto-clients"

	exit 1
fi

set -e

mqtt_host=${MQTT_HOST:-"localhost"}
mqtt_port=${MQTT_PORT:-1883}

topic_cs_josev="lamarrs/orchestrator"

send_orchestrator_message() {
	echo $(cat <<EOF
{
  "SendColor":
    {"color": "Red",
    "target_location": "Center"
  }
}
EOF
)
}

echo "Connecting to $mqtt_host:$mqtt_port"
mosquitto_pub -h $mqtt_host -p $mqtt_port -t $topic_cs_josev -m "$(send_orchestrator_message)"
