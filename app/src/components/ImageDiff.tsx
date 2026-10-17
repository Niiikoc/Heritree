import { useEffect, useState } from "react";
import type { RepoApi } from "../api";
import type { FileVersion } from "../types";

const MIME: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  bmp: "image/bmp",
  ico: "image/x-icon",
  svg: "image/svg+xml",
  avif: "image/avif",
};

export function imageMime(path: string): string | null {
  return MIME[path.split(".").pop()?.toLowerCase() ?? ""] ?? null;
}

interface Side {
  url: string;
  bytes: number;
  width?: number;
  height?: number;
}

interface Props {
  api: RepoApi;
  path: string;
  before: FileVersion | null;
  after: FileVersion | null;
  version: number;
}

/** Before and after of an image file, side by side. */
export function ImageDiff({ api, path, before, after, version }: Props) {
  const [sides, setSides] = useState<[Side | null, Side | null] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const mime = imageMime(path) ?? "application/octet-stream";

  useEffect(() => {
    let live = true;
    const urls: string[] = [];
    const load = async (v: FileVersion | null): Promise<Side | null> => {
      if (!v) return null;
      const buffer = await api.fileBytes(v);
      if (!buffer.byteLength) return null;
      const url = URL.createObjectURL(new Blob([buffer], { type: mime }));
      urls.push(url);
      return { url, bytes: buffer.byteLength };
    };
    Promise.all([load(before), load(after)]).then(
      (s) => live && setSides(s),
      (e) => live && setError(String(e)),
    );
    return () => {
      live = false;
      urls.forEach(URL.revokeObjectURL);
    };
  }, [api, before, after, mime, version]);

  if (error) return <p className="error pad">{error}</p>;
  if (!sides) return <p className="empty pad">Loading…</p>;

  const panel = (label: string, side: Side | null) => (
    <figure className="image-side">
      <figcaption>
        <strong>{label}</strong>{" "}
        {side ? (
          <span className="muted">
            {side.width ? `${side.width} × ${side.height} px · ` : ""}
            {formatBytes(side.bytes)}
          </span>
        ) : (
          <span className="muted">does not exist</span>
        )}
      </figcaption>
      <div className="checkerboard">
        {side && (
          <img
            src={side.url}
            alt={`${path}, ${label.toLowerCase()}`}
            onLoad={(e) => {
              const img = e.currentTarget;
              setSides((s) => {
                if (!s) return s;
                const next = [...s] as [Side | null, Side | null];
                const i = label === "Before" ? 0 : 1;
                if (next[i]) next[i] = { ...next[i]!, width: img.naturalWidth, height: img.naturalHeight };
                return next;
              });
            }}
          />
        )}
      </div>
    </figure>
  );

  return (
    <div className="image-diff">
      {panel("Before", sides[0])}
      {panel("After", sides[1])}
    </div>
  );
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
