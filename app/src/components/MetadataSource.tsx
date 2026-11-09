import { openMetadataSource, type CatalogRecord } from "../api";

export function MetadataSource({
  catalog,
  url,
}: Pick<CatalogRecord, "catalog" | "url">) {
  const metron = catalog === "metron";
  const label = metron ? "Metron" : "Mangainfo";
  return (
    <div className="metadata-source">
      <button className="link" onClick={() => void openMetadataSource(url)}>
        Data from {label}
      </button>
      {metron && (
        <button
          className="link"
          onClick={() =>
            void openMetadataSource(
              "https://creativecommons.org/licenses/by-sa/4.0/",
            )
          }
        >
          CC BY-SA 4.0
        </button>
      )}
    </div>
  );
}
