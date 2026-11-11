"""Strict trusted-review records and immutable execution evidence bindings."""
import hashlib
import json


def require(ok, why):
    if not ok:
        raise ValueError(why)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def valid_records(reviews):
    require(isinstance(reviews, list), 'Reviews must be a list')
    for review in reviews:
        require(isinstance(review, dict), 'Review must be an object')
        task = review.get('task_id')
        require(isinstance(task, str) and bool(task.strip()), 'Review task ID must be nonblank')
        reviewer = review.get('reviewer')
        require(isinstance(reviewer, dict), 'Reviewer identity must be structured')
        for field in ('provider', 'model'):
            value = reviewer.get(field)
            require(isinstance(value, str) and bool(value.strip()), 'Reviewer '+field+' must be nonblank')
    return reviews


def validate_approvals(reviews, commit, tree, results_digest, runner_digest, packager_digest, verifier_digest):
    records = valid_records(reviews)
    exact = [r for r in records if r.get('verdict') == 'PASS' and r.get('reviewed_commit') == commit and r.get('reviewed_tree') == tree]
    source = [r for r in exact if r.get('scope') == 'source']
    execution = [r for r in exact if r.get('scope') == 'execution_evidence' and r.get('full_results_sha256') == results_digest and r.get('executed_runner_sha256') == runner_digest]
    independent = [(a, b) for a in source for b in execution if a['task_id'] != b['task_id']]
    require(bool(independent), 'Independent source and exact execution approvals required')
    selected = dict(source=independent[0][0]['task_id'], execution=independent[0][1]['task_id'])
    for scope, expected in [('evidence_packaging', packager_digest), ('inheritance_verifier', verifier_digest)]:
        matches = [r for r in exact if r.get('scope') == scope and r.get('script_sha256') == expected]
        require(bool(matches), 'Exact '+scope+' design approval required')
        selected[scope] = matches[0]['task_id']
    return selected


def validate_committed_execution(manifest, results_bytes, runner_bytes, reviews_bytes, packager_bytes, verifier_bytes, commit, tree):
    results_digest = digest(results_bytes)
    runner_digest = digest(runner_bytes)
    results = json.loads(results_bytes)
    require(results_digest == manifest['full_results_sha256'], 'Committed results digest differs from manifest')
    require(results.get('commit') == commit and results.get('tree') == tree and results.get('status') == 'passed', 'Committed results name wrong tested source or incomplete run')
    require(results.get('runner_sha256') == runner_digest == manifest['executed_runner_sha256'], 'Committed executed runner differs')
    return validate_approvals(json.loads(reviews_bytes), commit, tree, results_digest, runner_digest, digest(packager_bytes), digest(verifier_bytes))


def validate_chain(assembly, objects, base, tested, tested_tree, plan, accepted):
    require(assembly.get('accepted') == accepted, 'Wrong accepted source identity')
    require(assembly.get('base') == base, 'Wrong assembly base')
    require(assembly.get('final') == tested and assembly.get('accepted_tree') == tested_tree and assembly.get('exact_tree_match') is True, 'Wrong assembly terminal source')
    units = assembly.get('units')
    require(isinstance(units, list) and len(units) == len(plan), 'Wrong assembly unit count')
    previous = base
    identity = 'Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>'
    for unit, (title, paths) in zip(units, plan):
        require(unit.get('source') == accepted, 'Wrong immutable unit source')
        require(unit.get('title') == title and unit.get('paths') == paths, 'Wrong ordered feature plan')
        require(unit.get('parent') == previous, 'Wrong recorded parent')
        obj = objects[unit['commit']]
        require(obj['parents'] == [previous], 'Expected exactly one immutable parent')
        require(obj['tree'] == unit.get('tree') and unit.get('remote') == unit['commit'], 'Wrong immutable unit identity')
        require(obj['author'] == obj['committer'] == identity, 'Wrong commit identity')
        require(set(obj['paths']) == set(paths), 'Wrong immutable changed paths')
        previous = unit['commit']
    require(previous == tested, 'Wrong last unit')


def validate_log_bindings(results, logs, read_blob):
    checks = results.get('checks')
    require(isinstance(checks, list) and isinstance(logs, list), 'Missing gate/log inventory')
    check_labels = [entry['label'] for entry in checks]
    log_labels = [entry['gate'] for entry in logs]
    require(len(check_labels) == len(set(check_labels)), 'Duplicate result gate')
    require(len(log_labels) == len(set(log_labels)), 'Duplicate archived gate')
    require(set(check_labels) == set(log_labels), 'Archived logs differ from approved gate inventory')
    expected = {entry['label']: entry['sha256'] for entry in checks}
    import gzip
    for entry in logs:
        compressed = read_blob(entry['path'])
        require(digest(compressed) == entry['archive_sha256'], 'Archived compressed log differs')
        raw_digest = digest(gzip.decompress(compressed))
        require(raw_digest == entry['raw_sha256'] == expected[entry['gate']], 'Archived log differs from approved execution record')



def validate_assembly_approval(reviews, commit, tree, assembly_bytes):
    records = valid_records(reviews)
    approvals = [r for r in records if r.get('scope') == 'assembly' and r.get('verdict') == 'PASS' and r.get('reviewed_commit') == commit and r.get('reviewed_tree') == tree and r.get('assembly_sha256') == digest(assembly_bytes)]
    require(bool(approvals), 'Exact independent assembly review digest required')
    return approvals[0]['task_id']


def validate_assembly_provenance(assembly_bytes, reviews_bytes, git, base, accepted, tested, tree, plan):
    assembly = json.loads(assembly_bytes)
    require(assembly.get('accepted') == accepted, 'Wrong accepted source identity')
    objects = {}
    for unit in assembly['units']:
        require(unit.get('source') == accepted, 'Wrong immutable unit source')
        commit = unit['commit']
        objects[commit] = dict(parents=git('rev-list','--parents','-n','1',commit).decode().split()[1:], tree=git('rev-parse',commit+'^{tree}').decode().strip(), author=git('show','-s','--format=%an <%ae>',commit).decode().strip(), committer=git('show','-s','--format=%cn <%ce>',commit).decode().strip(), paths=git('diff-tree','--no-commit-id','--name-only','-r',commit).decode().splitlines())
        require(git('ls-tree','-r','-z',commit,'--',*unit['paths']) == git('ls-tree','-r','-z',accepted,'--',*unit['paths']), 'Unit differs from pinned accepted source')
    validate_chain(assembly, objects, base, tested, tree, plan, accepted)
    source_map = git('ls-tree','-r','-z','--full-tree',tested)
    require(source_map == git('ls-tree','-r','-z','--full-tree',accepted) and digest(source_map) == assembly['complete_path_mode_blob_map_sha256'], 'Accepted source map differs')
    return validate_assembly_approval(json.loads(reviews_bytes), tested, tree, assembly_bytes)
