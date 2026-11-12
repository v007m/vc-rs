/**
 * `vc`'s coloured output as styled pieces: the SGR codes it writes (bold, dim and the 16
 * foreground colours) become class names, and any other escape is dropped. The page renders
 * the pieces as text, never as HTML.
 */

export interface Piece {
  text: string;
  /** Space-separated class names: `b` bold, `d` dim, `c31` … `c37` and `c91` … `c97` colours. */
  style: string;
}

interface Pen {
  bold: boolean;
  dim: boolean;
  colour: number | undefined;
}

const CSI = /\x1b\[([0-9;]*)([A-Za-z])/g;

function apply(pen: Pen, codes: string): void {
  for (const part of (codes || '0').split(';')) {
    const code = Number(part || '0');
    if (code === 0) Object.assign(pen, { bold: false, dim: false, colour: undefined });
    else if (code === 1) pen.bold = true;
    else if (code === 2) pen.dim = true;
    else if (code === 22) Object.assign(pen, { bold: false, dim: false });
    else if (code === 39) pen.colour = undefined;
    else if ((code >= 30 && code <= 37) || (code >= 90 && code <= 97)) pen.colour = code;
  }
}

function style(pen: Pen): string {
  return [pen.bold && 'b', pen.dim && 'd', pen.colour !== undefined && `c${pen.colour}`].filter(Boolean).join(' ');
}

/** `text` as pieces, joining neighbours of the same style. */
export function pieces(text: string): Piece[] {
  const out: Piece[] = [];
  const pen: Pen = { bold: false, dim: false, colour: undefined };
  const push = (piece: string) => {
    if (!piece) return;
    const current = style(pen);
    const last = out[out.length - 1];
    if (last && last.style === current) last.text += piece;
    else out.push({ text: piece, style: current });
  };
  let at = 0;
  for (const match of text.matchAll(CSI)) {
    push(text.slice(at, match.index));
    if (match[2] === 'm') apply(pen, match[1]);
    at = match.index + match[0].length;
  }
  push(text.slice(at).replace(/\x1b/g, ''));
  return out;
}
