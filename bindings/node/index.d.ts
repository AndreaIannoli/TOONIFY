export type SupportedFormat = "json" | "yaml" | "xml" | "csv" | "auto";
export type SupportedDelimiter = "comma" | "tab" | "pipe";
export type KeyFoldingMode = "off" | "safe";
export type PathExpansionMode = "off" | "safe";

export interface ConvertOptions {
    format?: SupportedFormat;
    delimiter?: SupportedDelimiter;
    indent?: number;
    keyFolding?: KeyFoldingMode;
    flattenDepth?: number;
}

export interface DecodeOptions {
    indent?: number;
    expandPaths?: PathExpansionMode;
    loose?: boolean;
    pretty?: boolean;
}

export function convertToToon(input: string, options?: ConvertOptions): string;
export function decodeToJson(input: string, options?: DecodeOptions): string;
export function validateToon(input: string, options?: DecodeOptions): void;
export function version(): string;
