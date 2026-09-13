/* What every Clermont-Ferrand study shares regardless of trade: the frame, and the location Google
   resolves search volume against. The rest is [_France.nix]. */
let france = import ./_France.nix; in
france
  // {
  area = {
    # Clermont-Ferrand agglomeration, generous: Riom (N) to Issoire-ward (S), Volvic (W) to Lezoux-ward (E).
    bbox = { lat = [ 45.55 45.95 ]; lon = [ 2.90 3.40 ]; };
    center = [ 45.7797 3.0863 ];
    zoom = 12;
  };

  searches = france.searches // { place = "Clermont-Ferrand,Auvergne-Rhone-Alpes,France"; };
}
