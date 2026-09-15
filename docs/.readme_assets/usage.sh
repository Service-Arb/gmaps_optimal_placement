# Set your Google Maps API key. The server reads it; it never lands in a file.
export GOOGLE_MAPS_KEY=...

# Serve the map. A study is a trade applied to a location, so give the two directories. The page
# shows a picker: the trade first, then the city. Each pairing you select is a tab over one map.
# Press `t` to open one more.
# Bulk archives and API responses cache in $GMAPS_OPTIMAL_PLACEMENT_WORK (default ./tmp/geo);
# candidates you promote from the map land in $XDG_DATA_HOME/gmaps_optimal_placement.
gmaps_optimal_placement serve examples/trades examples/locations --open

# Name both as files to build one pairing before the server starts.
gmaps_optimal_placement serve examples/trades/cleaning.nix examples/locations/Lyon.nix

# Chart the monthly search volume of each query group. Set the credentials of the provider first:
# GOOGLE_ADS_{DEVELOPER_TOKEN,CLIENT_ID,CLIENT_SECRET,REFRESH_TOKEN,CUSTOMER_ID}, or
# DATAFORSEO_{LOGIN,PASSWORD}. A directory is picked through fzf.
gmaps_optimal_placement searches examples/trades examples/locations

# Show the schema of the study document.
gmaps_optimal_placement schema

# Find the towns in a country that are worth a study. Count what a cadastre draws, per commune, and
# divide by a column of the grid. Towns below the floor are not counted, and not downloaded.
gmaps_optimal_placement misc france pool --per ind --floor 2000
