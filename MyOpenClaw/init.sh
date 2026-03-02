#!/bin/bash

# Define colors for output
GREEN='\033[0;32m'
NC='\033[0m' # No Color

echo -e "${GREEN}Configuring OpenClaw Docker...${NC}"

# Check for .env file
if [ ! -f .env ]; then
    echo -e "${GREEN}Creating .env file from .env.example...${NC}"
    cp .env.example .env

    # Generate random token using openssl or fallback to dev token
    TOKEN=$(openssl rand -hex 32 2>/dev/null || echo "dev-token-$(date +%s)")
    
    # Update TOKEN in .env
    sed -i "s/OPENCLAW_GATEWAY_TOKEN=your_random_token_here/OPENCLAW_GATEWAY_TOKEN=$TOKEN/g" .env
    echo -e "${GREEN}Generated random security token.${NC}"
fi

# Create persistent storage directories
mkdir -p ./config ./workspace
chmod 777 ./config ./workspace
echo -e "${GREEN}Created config and workspace directories.${NC}"

echo -e "\n${GREEN}Initialization complete!${NC}"
echo "You can now run:"
echo "  docker-compose up -d"
echo -e "\n${GREEN}Important:${NC} Make sure to check the .env file for more configuration options."
