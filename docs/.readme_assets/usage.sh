# Set your Google Maps API key. The map embeds it.
export GOOGLE_MAPS_KEY=...

# Write the map. Bulk archives and API responses cache in $SERVICE_ARB_WORK (default ./tmp/geo).
service_arb map examples/clermont_detailing/config.nix

# Chart the monthly search volume of each query group. Set the credentials of the provider first:
# GOOGLE_ADS_{DEVELOPER_TOKEN,CLIENT_ID,CLIENT_SECRET,REFRESH_TOKEN,CUSTOMER_ID}, or
# DATAFORSEO_{LOGIN,PASSWORD}.
service_arb searches examples/clermont_detailing/config.nix

# Show the schema of the study document.
service_arb schema
