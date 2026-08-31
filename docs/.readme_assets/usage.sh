# Set your Google Maps API key. The map embeds it.
export GOOGLE_MAPS_KEY=...

# Write the map. Bulk archives and API responses cache in $SERVICE_ARB_WORK (default ./tmp/geo).
service_arb examples/clermont_detailing/config.nix

# Show the schema of the study document.
service_arb --schema
