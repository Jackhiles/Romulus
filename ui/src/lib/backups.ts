// Backup folder names are "<YYYY-MM-DD_HH-MM-SS>-<reason>" (sync crate, run::stamp).
const REASONS: Record<string, string> = {
  'before-download': 'before RomM’s save was downloaded',
  'romm-version': 'RomM’s version, replaced by this Deck’s',
  'before-restore': 'before a backup was restored',
  'before-card-format': 'before the memory card was formatted',
}

export function describeBackup(label: string): { when: string; why: string } {
  const m = /^(\d{4})-(\d\d)-(\d\d)_(\d\d)-(\d\d)-(\d\d)-(.+)$/.exec(label)
  if (!m) return { when: label, why: '' }
  const d = new Date(Date.UTC(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6]))
  return {
    when: d.toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' }),
    why: REASONS[m[7]] ?? m[7].replace(/-/g, ' '),
  }
}
