import { useCallback, useState, useRef, useEffect } from 'react';

export interface DroppedFile {
  id: string;
  path: string;
  name: string;
  type: string;
  isImage: boolean;
  dataUrl?: string; // For image previews
  isLoading?: boolean;
  error?: string;
}

export const useFileDrop = () => {
  const [droppedFiles, setDroppedFiles] = useState<DroppedFile[]>([]);
  const activeReadersRef = useRef<Set<FileReader>>(new Set());

  // Cleanup effect to prevent memory leaks
  useEffect(() => {
    return () => {
      // Abort any active FileReaders on unmount
      // eslint-disable-next-line react-hooks/exhaustive-deps
      const readers = activeReadersRef.current;
      readers.forEach((reader) => {
        try {
          reader.abort();
        } catch {
          // Reader might already be done, ignore errors
        }
      });
      readers.clear();
    };
  }, []);

  const handleDrop = useCallback(async (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    const files = e.dataTransfer.files;
    if (files.length > 0) {
      const droppedFileObjects: DroppedFile[] = [];

      for (let i = 0; i < files.length; i++) {
        const file = files[i];

        let droppedFile: DroppedFile;

        try {
          const path = window.electron.getPathForFile(file);
          const isImage = file.type.startsWith('image/');

          droppedFile = {
            id: `dropped-${Date.now()}-${i}`,
            path,
            name: file.name,
            type: file.type,
            isImage,
            isLoading: isImage, // Only images need loading state for preview generation
          };
        } catch (error) {
          console.error('Error processing file:', file.name, error);
          // Create an error file object
          droppedFile = {
            id: `dropped-error-${Date.now()}-${i}`,
            path: '',
            name: file.name,
            type: file.type,
            isImage: false,
            isLoading: false,
            error: `Failed to get file path: ${error instanceof Error ? error.message : 'Unknown error'}`,
          };
        }

        droppedFileObjects.push(droppedFile);

        // For images, generate a preview AND persist a temp copy so the send
        // path can read the bytes through the same temp-dir IPC the paste flow
        // uses. The OS-supplied source path is rejected by the IPC's path
        // validation (and may be absent altogether for synthetic File objects),
        // so we cannot rely on it for the model-bound base64 read.
        if (droppedFile.isImage && !droppedFile.error) {
          const reader = new FileReader();
          activeReadersRef.current.add(reader);

          reader.onload = async (event) => {
            const dataUrl = event.target?.result as string;
            try {
              const saved = await window.electron.saveDataUrlToTemp(dataUrl, droppedFile.id);
              if (saved.error || !saved.filePath) {
                throw new Error(saved.error ?? 'saveDataUrlToTemp returned no path');
              }
              setDroppedFiles((prev) =>
                prev.map((f) =>
                  f.id === droppedFile.id
                    ? { ...f, dataUrl, path: saved.filePath as string, isLoading: false }
                    : f
                )
              );
            } catch (err) {
              setDroppedFiles((prev) =>
                prev.map((f) =>
                  f.id === droppedFile.id
                    ? {
                        ...f,
                        dataUrl,
                        isLoading: false,
                        error: `Failed to stage image: ${err instanceof Error ? err.message : String(err)}`,
                      }
                    : f
                )
              );
            } finally {
              activeReadersRef.current.delete(reader);
            }
          };

          reader.onerror = () => {
            console.error('Failed to generate preview for:', file.name);
            setDroppedFiles((prev) =>
              prev.map((f) =>
                f.id === droppedFile.id
                  ? { ...f, error: 'Failed to load image preview', isLoading: false }
                  : f
              )
            );
            activeReadersRef.current.delete(reader);
          };

          reader.onabort = () => {
            activeReadersRef.current.delete(reader);
          };

          reader.readAsDataURL(file);
        }
      }

      setDroppedFiles((prev) => [...prev, ...droppedFileObjects]);
    }
  }, []);

  const handleDragOver = useCallback((e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
  }, []);

  return {
    droppedFiles,
    setDroppedFiles,
    handleDrop,
    handleDragOver,
  };
};
