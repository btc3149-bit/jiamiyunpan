/**
 * mammoth ships no declaration files. Only the docx preview consumes it,
 * and only this one call — keep the ambient surface as small as what the
 * component actually uses.
 */
declare module 'mammoth' {
  export interface MammothMessage {
    type: string
    message: string
  }

  export interface MammothResult {
    /** Sanitized-on-arrival HTML fragment produced from the document body. */
    value: string
    messages: MammothMessage[]
  }

  export function convertToHtml(
    input: { buffer: Uint8Array },
    options?: Record<string, unknown>
  ): Promise<MammothResult>
}
