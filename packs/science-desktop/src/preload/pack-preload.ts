/**
 * Minimal preload for pack proof — no extra bridges.
 */
import { contextBridge } from 'electron'

contextBridge.exposeInMainWorld('lumenPack', {
  productName: 'Lumen Science Desktop',
  packProof: true,
})
