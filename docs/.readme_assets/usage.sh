# Set your Google Maps API key. The server reads it; it never lands in a file.
export GOOGLE_MAPS_KEY=...

# Serve the map. Bulk archives and API responses cache in $GMAPS_OPTIMAL_PLACEMENT_WORK (default ./tmp/geo);
# candidates you promote from the map land in $XDG_DATA_HOME/gmaps_optimal_placement.
gmaps_optimal_placement serve examples/car_detailing_-_Clermont-Ferrand.nix --open

# Chart the monthly search volume of each query group. Set the credentials of the provider first:
# GOOGLE_ADS_{DEVELOPER_TOKEN,CLIENT_ID,CLIENT_SECRET,REFRESH_TOKEN,CUSTOMER_ID}, or
# DATAFORSEO_{LOGIN,PASSWORD}.
gmaps_optimal_placement searches examples/car_detailing_-_Clermont-Ferrand.nix

# Show the schema of the study document.
gmaps_optimal_placement schema
