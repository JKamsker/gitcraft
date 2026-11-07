"""Offline correctness check using generated fixture keys only; no user configuration."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

SOPS = Path('/workspace/work/sops-fixture-tools/sops')
AGE = Path('/workspace/work/sops-fixture-tools/age-keygen')

with tempfile.TemporaryDirectory(prefix='gitcraft-review-sops-format-') as root:
    home = Path(root)
    os.chmod(home, 0o700)
    key = home / 'generated-fixture-age-key.txt'
    env = {'PATH': '/usr/bin:/bin', 'HOME': root,
           'XDG_CONFIG_HOME': str(home / 'config'),
           'XDG_CACHE_HOME': str(home / 'cache'),
           'SOPS_DECRYPTION_ORDER': 'age', 'SOPS_AGE_KEY_FILE': str(key)}

    def run(args, data=None):
        return subprocess.run(args, input=data, env=env, cwd=home,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                              timeout=5, check=False)

    assert run([AGE, '-o', key]).returncode == 0
    public = run([AGE, '-y', key])
    assert public.returncode == 0
    recipient = public.stdout.decode().strip()
    assert recipient.startswith('age1')

    def encrypt(kind, data):
        out = run([SOPS, 'encrypt', '--input-type', kind, '--output-type', kind,
                   '--age', recipient, '--filename-override', 'gitcraft.fixture'], data)
        assert out.returncode == 0
        return out.stdout

    def decrypt(kind, data):
        return run([SOPS, 'decrypt', '--input-type', kind, '--output-type', kind,
                    '--filename-override', 'gitcraft.fixture'], data)

    # model::inspect picks Json for this exact envelope whenever the path ends .json.
    binary = encrypt('binary', b'fixture\0binary')
    as_binary = decrypt('binary', binary)
    as_json = decrypt('json', binary)
    assert as_binary.returncode == as_json.returncode == 0
    assert b'\0' in as_binary.stdout and b'\0' not in as_json.stdout
    assert json.loads(as_json.stdout) == {'data': 'fixture\0binary'}
    # model::inspect picks Binary for this valid structured JSON at an extensionless path.
    structured = encrypt('json', b'{"data":42}\n')
    object_keys = sorted(json.loads(structured))
    assert object_keys == ['data', 'sops']
    valid_json = decrypt('json', structured)
    wrong_binary = decrypt('binary', structured)
    assert valid_json.returncode == 0 and json.loads(valid_json.stdout) == {'data': 42}
    print(json.dumps({
        'fixture_only': True,
        'binary_envelope_json_route_returns_json_instead_of_binary': True,
        'json_route_escapes_nul_instead_of_exposing_binary_refusal': True,
        'single_data_field_json_is_ambiguous_with_binary_envelope': True,
        'single_data_field_integer_json_binary_route_exit': wrong_binary.returncode,
        'single_data_field_integer_json_binary_route_matches_json':
            wrong_binary.returncode == 0 and wrong_binary.stdout == valid_json.stdout,
        'raw_tool_diagnostics_printed': False,
    }, sort_keys=True))
