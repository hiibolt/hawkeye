FROM nixos/nix:2.18.3 AS builder

# Set up the environment for Nix Flakes
RUN nix-channel --update
RUN echo "experimental-features = nix-command flakes" >> /etc/nix/nix.conf

# Set up the environment for the Nix Flake
WORKDIR /app
COPY flake.nix flake.lock Cargo.lock ./

# Cache the dependencies
RUN nix develop .#hawkeye

# Import the work directory and build
COPY . .
RUN nix bundle --bundler github:ralismark/nix-appimage .#hawkeye

# Copy the AppImage and `public` dir to the final stage
FROM scratch
COPY --from=builder /app/hawkeye.AppImage /app/hawkeye.AppImage
COPY --from=builder /app/public /app/public

# Run the binary
VOLUME /data
VOLUME /root/.ssh
CMD ["/app/hawkeye.AppImage"]
EXPOSE 5777

# Reminder: Command to run this image with terminal is `docker run -it <image> /bin/bash`