// Run the browser tests.
//
//   SFM_PASSWORD=... node tests/run.mjs
//
// The tests need the app running: the frontend on :15173 and the
// backend on :8000. run_dev.sh starts both.

import { run } from './harness.mjs'
import pictureSearch from './picture-search.test.mjs'
import indexControls from './index-controls.test.mjs'
import pages from './pages.test.mjs'
import fileManager from './file-manager.test.mjs'
import uploadTrash from './upload-trash.test.mjs'
import errors from './errors.test.mjs'
import publicShare from './public-share.test.mjs'
import chat from './chat.test.mjs'

const failures = await run([pictureSearch, indexControls, pages, fileManager, uploadTrash, errors, publicShare, chat])
process.exit(failures ? 1 : 0)
