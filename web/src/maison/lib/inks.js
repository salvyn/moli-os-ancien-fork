// A paper printer's cartridges, as its points name them (`ink_N`, the
// driver's words in the house's language): whether it prints in colour.

const COLOUR = /couleur|colou?r|cyan|magenta|jaune|yellow|photo/i;

/** Black and white only: it says its cartridges and none is a colour (a
 *  laser's black toner and its drum). Unknown cartridges: maybe colour. */
export const monochrome = (labels) => labels.length > 0 && !labels.some((l) => COLOUR.test(String(l ?? '')));
