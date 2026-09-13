/* What every Lyon study shares regardless of trade: the frame, and the location Google resolves
   search volume against. The rest is [_France.nix]. */
let france = import ./_France.nix; in
france
  // {
  area = {
    # The Métropole and the ring that commutes into it: Neuville (N) to Givors (S), the Monts du
    # Lyonnais (W) to Pont-de-Chéruy-ward (E). Villefranche-sur-Saône is left out — it is 30 km up
    # the Saône and buys from its own high street.
    bbox = { lat = [ 45.62 45.90 ]; lon = [ 4.70 5.02 ]; };
    center = [ 45.7578 4.8320 ];
    zoom = 12;
  };

  searches = france.searches // { place = "Lyon,Auvergne-Rhone-Alpes,France"; };
}
