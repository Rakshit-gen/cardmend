import { useRef, useState } from "react";

// Accepts files dropped anywhere on the area or picked with the button.
export default function DropZone({
  onFiles,
  busy,
  compact = false,
}: {
  onFiles: (files: File[]) => void;
  busy: string | null;
  compact?: boolean;
}) {
  const input = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);

  return (
    <div
      className={`drop${over ? " over" : ""}${compact ? " compact" : ""}`}
      onDragOver={(e) => {
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        if (!busy && e.dataTransfer.files.length) onFiles([...e.dataTransfer.files]);
      }}
    >
      {busy ? (
        <p className="busy" role="status">
          {busy}
        </p>
      ) : (
        <>
          {!compact && (
            <p>
              Drop your contacts exports here: <code>.vcf</code> files from iPhone, iCloud,
              Android or Outlook, and Google or Outlook CSV files. Drop all of them together to
              find duplicates across them.
            </p>
          )}
          <button type="button" onClick={() => input.current?.click()}>
            {compact ? "Add another file" : "Choose files"}
          </button>
        </>
      )}
      <input
        ref={input}
        type="file"
        multiple
        accept=".vcf,.vcard,.csv,text/vcard,text/csv"
        hidden
        onChange={(e) => {
          const files = [...(e.target.files ?? [])];
          e.target.value = "";
          if (files.length) onFiles(files);
        }}
      />
    </div>
  );
}
