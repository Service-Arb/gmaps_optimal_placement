# Set your Google Maps API key. The server reads it; it never lands in a file.
export GOOGLE_MAPS_KEY=...

# Serve the map. Bulk archives and API responses cache in $SERVICE_ARB_WORK (default ./tmp/geo);
# candidates you promote from the map land in $XDG_DATA_HOME/service_arb.
service_arb serve examples/car_detailing_-_Clermont-Ferrand.nix --open

# Chart the monthly search volume of each query group. Set the credentials of the provider first:
# GOOGLE_ADS_{DEVELOPER_TOKEN,CLIENT_ID,CLIENT_SECRET,REFRESH_TOKEN,CUSTOMER_ID}, or
# DATAFORSEO_{LOGIN,PASSWORD}.
service_arb searches examples/car_detailing_-_Clermont-Ferrand.nix

# Show the schema of the study document.
service_arb schema
