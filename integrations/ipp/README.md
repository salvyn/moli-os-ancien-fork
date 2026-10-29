# Imprimante réseau (IPP)

- IPP 2.0 en HTTP sur 631 : état (`printer-state` + raisons en mots), niveau de chaque
  cartouche (`marker-levels`, codes négatifs = inconnu), file d'attente, toutes les 30 s
  (4 s pendant une impression).
- Imprimer : le tableau de bord envoie des pages JPEG (un PDF est rendu page par page dans
  le navigateur) ; Moli les met en file et les envoie une à une (`Print-Job`, 5 min max : une
  jet d'encre lit le document en imprimant). Couleur ou noir, copies, recto-verso si l'imprimante
  sait le faire (`sides-supported`).
- Une laser Brother qui ne prend que son langage (`application/vnd.brother-hbp`, HL-1210W…) :
  le tableau de bord envoie des feuilles en points (PBM à 600 ppp : format, 1, 2 ou 4 pages par
  feuille, taille réelle, recto verso à la main) et Moli les dit dans ce langage (`src/hbp.rs` :
  papier, type de papier, économie de toner, densité). Écrit d'après la description du format
  par le pilote libre brlaser, sans son code.
- Éteinte au bouton : hors ligne, journalisé en info (pas une panne).
- Fixture : la vraie réponse d'une Canon MG3600 (`fixtures/mg3600-attributes.ipp`), lue par
  les tests de `crates/moli-ipp/src/proto.rs`.
