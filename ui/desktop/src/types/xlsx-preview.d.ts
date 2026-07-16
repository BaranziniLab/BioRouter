declare module 'xlsx-preview' {
  export type XlsxPreviewInput = Blob | File | ArrayBuffer;

  export type XlsxPreviewOptions = {
    output?: 'string' | 'arrayBuffer';
    separateSheets?: boolean;
    minimumRows?: number;
    minimumCols?: number;
  };

  export function xlsx2Html(
    data: XlsxPreviewInput,
    options?: XlsxPreviewOptions
  ): Promise<string | ArrayBuffer | string[] | Promise<ArrayBuffer>[]>;
}
