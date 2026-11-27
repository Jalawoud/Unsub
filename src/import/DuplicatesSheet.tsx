import { useEffect, useState } from "react";
import { libraryAddAnyway, onDuplicatesHeld } from "../api/commands";
import type { HeldDuplicate } from "../api/types";
import { formatClock, pluralize } from "../util/format";
import "./DuplicatesSheet.css";

const quality = (codec: string, kbps: number | null, ms: number) =>
  [codec, kbps ? `${kbps} kbps` : null, formatClock(ms)].filter(Boolean).join(" · ");

const fileName = (path: string) => path.slice(path.lastIndexOf("/") + 1);

/**
 * Asks about files an import held back because the library already has those songs (same title,
 * artist and album, lengths within 15 seconds). Skipping leaves the files where they are, unadded;
 * adding imports them without checking again.
 */
export function DuplicatesSheet() {
  const [held, setHeld] = useState<HeldDuplicate[]>([]);
  const [chosen, setChosen] = useState<Set<string>>(new Set());

  useEffect(() => {
    const subscription = onDuplicatesHeld((more) =>
      // A second import can finish while this is open; its files join the list.
      setHeld((current) => [...current, ...more.filter((file) => !current.some((c) => c.path === file.path))]),
    );
    return () => void subscription.then((unlisten) => unlisten());
  }, []);

  if (held.length === 0) return null;
  const close = () => {
    setHeld([]);
    setChosen(new Set());
  };
  const add = (paths: string[]) => {
    close();
    if (paths.length > 0) void libraryAddAnyway(paths);
  };
  const toggle = (path: string) =>
    setChosen((current) => {
      const next = new Set(current);
      if (!next.delete(path)) next.add(path);
      return next;
    });
  const count = held.length;

  return (
    <div className="sheet-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="sheet duplicates-sheet" role="dialog" aria-label="Duplicate Songs">
        <h2 className="first-sync-title">
          {count === 1 ? "This song is" : `${count} songs are`} already in your library.
        </h2>
        <p className="prefs-note">
          {count === 1 ? "It has" : "Each has"} the same title, artist and album as a song you have, and is within 15
          seconds of its length. Skipped files stay where they are and aren't added.
        </p>
        <div className="duplicates-list" tabIndex={0}>
          {held.map((file) => (
            <label key={file.path} className="duplicates-row" title={`${file.path}\nmatches ${file.existing.path}`}>
              <input type="checkbox" checked={chosen.has(file.path)} onChange={() => toggle(file.path)} />
              <span className="duplicates-song">
                <strong>{file.title}</strong> — {file.artist}
                <span className="duplicates-album">{file.album}</span>
              </span>
              <span className="duplicates-versions">
                <span>New: {quality(file.codec, file.bitrateKbps, file.durationMs)}</span>
                <span className="duplicates-file">{fileName(file.path)}</span>
                <span>
                  In library:{" "}
                  {quality(file.existing.codec, file.existing.bitrateKbps, file.existing.durationMs)}
                </span>
                <span className="duplicates-file">{fileName(file.existing.path)}</span>
              </span>
            </label>
          ))}
        </div>
        <div className="sheet-buttons">
          <button className="push-button" onClick={() => add(held.map((file) => file.path))}>
            Add All Anyway
          </button>
          <button className="push-button" disabled={chosen.size === 0} onClick={() => add([...chosen])}>
            Add {chosen.size > 0 ? `${chosen.size} ` : ""}Checked
          </button>
          <button className="push-button push-button--default" autoFocus onClick={close}>
            Skip {pluralize(count, "Duplicate", "Duplicates")}
          </button>
        </div>
      </div>
    </div>
  );
}
