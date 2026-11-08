# Evidence storage

Generated binary and compressed evidence artifacts are not included in this change. Plain-text review reports and JSON source/hash records remain. Earlier manifests retain historical filenames and hashes; those artifacts are external evidence, not repository files.

The integration branch removes 1,229 compressed archives introduced by issue51 work. Their local preservation index is `/tmp/gitcraft-51-evidence-archives/index.json`. The final branch will be assembled from fresh fork dev without these archives in any feature commit. Existing unrelated baseline artifacts are not additions to this PR. No application source or validation outcome changed in this documentation cleanup.
