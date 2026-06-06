'use client';

import { useState, useEffect, useRef, useCallback } from 'react';
import { Document, Page, pdfjs } from 'react-pdf';
import type { Citation } from '@/types/documentQa';

// Configure PDF.js worker for client-side rendering
if (typeof window !== 'undefined') {
  pdfjs.GlobalWorkerOptions.workerSrc = new URL(
    'pdfjs-dist/build/pdf.worker.min.mjs',
    import.meta.url,
  ).toString();
}

interface PdfViewerProps {
  file: string | null;
  targetPage?: number;
  highlight?: Citation | null;
  onHighlightConsumed?: () => void;
}

export function PdfViewer({
  file,
  targetPage,
  highlight,
  onHighlightConsumed,
}: PdfViewerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [pageNumber, setPageNumber] = useState(1);
  const [numPages, setNumPages] = useState(0);
  const [containerWidth, setContainerWidth] = useState(500);
  const [pageOriginalHeight, setPageOriginalHeight] = useState(792);
  const [pageOriginalWidth, setPageOriginalWidth] = useState(612);
  const [highlightVisible, setHighlightVisible] = useState(true);

  // Measure container width for responsive scaling
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setContainerWidth(entry.contentRect.width);
      }
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // Navigate to the cited page when a citation is clicked
  useEffect(() => {
    if (targetPage && targetPage >= 1) {
      setPageNumber(targetPage);
    }
  }, [targetPage]);

  // Show highlight then auto-fade after 3 seconds
  useEffect(() => {
    if (!highlight) return;
    setHighlightVisible(true);
    const timer = setTimeout(() => {
      setHighlightVisible(false);
      onHighlightConsumed?.();
    }, 3000);
    return () => clearTimeout(timer);
  }, [highlight, onHighlightConsumed]);

  const onDocumentLoadSuccess = useCallback(
    ({ numPages }: { numPages: number }) => {
      setNumPages(numPages);
      if (targetPage && targetPage >= 1 && targetPage <= numPages) {
        setPageNumber(targetPage);
      } else {
        setPageNumber(1);
      }
    },
    [targetPage],
  );

  const onPageRenderSuccess = useCallback((page: any) => {
    if (page?.originalHeight) setPageOriginalHeight(page.originalHeight);
    if (page?.originalWidth) setPageOriginalWidth(page.originalWidth);
  }, []);

  if (!file) return null;

  // Scale to fit container width
  const scale = containerWidth / pageOriginalWidth;

  const showHighlight =
    highlight &&
    highlight.page === pageNumber &&
    highlightVisible &&
    highlight.bbox.length === 4;

  // Convert PDF coordinate system (origin bottom-left, y-up) to screen
  // coordinates (origin top-left, y-down).
  let overlayStyle: React.CSSProperties | undefined;
  if (showHighlight) {
    const [x1, _y1, x2, y2] = highlight!.bbox;
    overlayStyle = {
      position: 'absolute',
      left: x1 * scale,
      top: (pageOriginalHeight - y2) * scale,
      width: (x2 - x1) * scale,
      height: (y2 - highlight!.bbox[1]) * scale,
      backgroundColor: 'rgba(253, 224, 71, 0.4)',
      outline: '2px solid #eab308',
      pointerEvents: 'none',
      borderRadius: '2px',
      zIndex: 10,
    };
  }

  return (
    <div ref={containerRef} className="flex flex-col h-full">
      {/* PDF rendering area */}
      <div className="flex-1 overflow-auto flex justify-center bg-slate-100 dark:bg-slate-900">
        <Document
          file={file}
          onLoadSuccess={onDocumentLoadSuccess}
          loading={
            <div className="flex items-center justify-center py-20">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500" />
            </div>
          }
        >
          <div className="relative inline-block">
            <Page
              pageNumber={pageNumber}
              width={containerWidth}
              onRenderSuccess={onPageRenderSuccess}
              renderTextLayer={false}
              renderAnnotationLayer={false}
            />
            {showHighlight && overlayStyle && <div style={overlayStyle} />}
          </div>
        </Document>
      </div>

      {/* Page navigation */}
      {numPages > 1 && (
        <div className="flex items-center justify-center gap-3 py-2 px-3 border-t border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 shrink-0">
          <button
            onClick={() => setPageNumber((p) => Math.max(1, p - 1))}
            disabled={pageNumber <= 1}
            className="px-3 py-1 text-sm bg-slate-100 dark:bg-slate-700 text-slate-700 dark:text-slate-200 rounded disabled:opacity-50 hover:bg-slate-200 dark:hover:bg-slate-600 transition-colors"
          >
            Prev
          </button>
          <span className="text-sm text-slate-600 dark:text-slate-400">
            Page {pageNumber} / {numPages}
          </span>
          <button
            onClick={() => setPageNumber((p) => Math.min(numPages, p + 1))}
            disabled={pageNumber >= numPages}
            className="px-3 py-1 text-sm bg-slate-100 dark:bg-slate-700 text-slate-700 dark:text-slate-200 rounded disabled:opacity-50 hover:bg-slate-200 dark:hover:bg-slate-600 transition-colors"
          >
            Next
          </button>
        </div>
      )}
    </div>
  );
}
