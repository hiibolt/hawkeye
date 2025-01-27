FROM nixos/nix:2.18.3 AS builder

# Set up the environment for Nix Flakes
RUN nix-channel --update
RUN echo "experimental-features = nix-command flakes" >> /etc/nix/nix.conf

# Set up the environment for the Nix Flake and build
WORKDIR /app
COPY . .
RUN nix build .#hawkeye

# Run the binary
VOLUME /data
VOLUME /root/.ssh
CMD ["/app/result/bin/hawkeye"]
EXPOSE 5777

# Reminder: Command to run this image with terminal is `docker run -it <image> /bin/bash`