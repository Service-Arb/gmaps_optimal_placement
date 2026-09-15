/* What every Clermont-Ferrand study shares regardless of trade: the frame, the location Google
   resolves search volume against, and the premises worth asking about. The rest is
   [../_France.nix]. */
let france = import ../_France.nix; in
france
  // {
  area = {
    # Clermont-Ferrand agglomeration, generous: Riom (N) to Issoire-ward (S), Volvic (W) to Lezoux-ward (E).
    bbox = { lat = [ 45.55 45.95 ]; lon = [ 2.90 3.40 ]; };
    center = [ 45.7797 3.0863 ];
    zoom = 12;
  };

  searches = france.searches // { place = "Clermont-Ferrand,Auvergne-Rhone-Alpes,France"; };

  # A premises, not a business: whichever trade moves into it, it is the same unit on the same street.
  candidate = [
    { name = "VifNet"; at = [ 45.77616 3.06373 ]; }
  ];
}
