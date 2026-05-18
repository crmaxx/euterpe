import { Link } from "react-router-dom";

export function LibraryPage() {
  return (
    <div className="mx-auto max-w-lg space-y-4">
      <h2 className="text-2xl font-semibold">Library</h2>
      <p className="text-muted-foreground">
        Full library browser — Phase 5 (filesystem rescan, tags, covers).
      </p>
      <p>
        <Link to="/favorites" className="text-zinc-300 underline">
          Browse Qobuz favorites
        </Link>
      </p>
    </div>
  );
}
