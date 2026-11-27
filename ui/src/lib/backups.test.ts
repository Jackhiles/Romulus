import { describe, expect, it } from 'vitest'
import { describeBackup } from './backups'

describe('describeBackup', () => {
  it('reads the time and reason', () => {
    const b = describeBackup('2026-11-27_05-20-33-before-download')
    expect(b.why).toBe('before RomM’s save was downloaded')
    expect(b.when).toContain('2026')
    expect(describeBackup('2026-11-27_05-20-33-something-new').why).toBe('something new')
    expect(describeBackup('odd')).toEqual({ when: 'odd', why: '' })
  })
})
