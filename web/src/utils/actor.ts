/** Audit actors are stored as `user:<username>` (or `system:…`); people only need the name. */
export function actorName(actor: string | null | undefined): string {
  return (actor ?? "").replace(/^user:/, "");
}
