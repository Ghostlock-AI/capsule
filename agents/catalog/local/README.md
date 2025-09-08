# local agent

simple local research agent meant to test
capsules networking monitoring

### setup

```bash
# create .env file
touch .env

# open env file
vim .env

# in the .env file paste openai api key
OPENAI_API_KEY={your_key}

# save and exit
:x
```

### running

```bash
# classic python setup
source .venv/bin/activate
pip install -r requirements.txt
python3 agent.py
```
