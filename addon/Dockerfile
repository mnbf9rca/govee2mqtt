ARG BUILD_FROM=ghcr.io/home-assistant/base-debian:bookworm
FROM ghcr.io/mnbf9rca/govee2mqtt:latest AS govee2mqtt
FROM $BUILD_FROM
LABEL io.hass.type="addon" \
      org.opencontainers.image.title="Home Assistant Add-on: Govee to MQTT Bridge" \
      org.opencontainers.image.description="Acts as a bridge between Govee devices and Home Assistant, via the Home Assistant MQTT Integration." \
      org.opencontainers.image.source="https://github.com/mnbf9rca/govee2mqtt" \
      org.opencontainers.image.licenses="MIT"
COPY run.sh /run.sh
COPY --from=govee2mqtt /app/govee /app/
COPY --from=govee2mqtt /app/assets /app/assets/
COPY --from=govee2mqtt /app/AmazonRootCA1.pem /app/
CMD [ "/run.sh" ]
