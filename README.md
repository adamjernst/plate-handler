# Plate Handler

Links [Plate Recognizer](https://platerecognizer.com)'s [Stream API](https://platerecognizer.com/stream/) to [Home Assistant](https://www.home-assistant.io).

![Screenshot of a push notification](img/notif.jpeg)

- Receive [notifications](https://companion.home-assistant.io/docs/notifications/notifications-basic) when plates are spotted.
- Associate names with plates.
- Automatic [logbook](https://www.home-assistant.io/integrations/logbook/) entries and [events](https://www.home-assistant.io/docs/configuration/events/) for spotted plates.

## Setup

If you're using Docker Compose, set up plate-handler as a service:

```
version: 3.1
services:
  plate-handler:
    container_name: plate-handler
    restart: unless-stopped
    # plate-handler connects to Home Assistant on port 8123;
    # plate-recognizer connects to plate-handler via a webhook
    # on port 8402. Host networking is simplest, but less secure.
    network_mode: "host"
    volumes:
      - /home/username/.homeassistant/www/plates:/plates
      - /var/plate-recognizer:/data
    environment:
      ACCESS_TOKEN: "generate-from-home-assistant"
      PLATES_URL: "https://yoursite.ui.nabu.casa/local/plates/"
```

Generate the `ACCESS_TOKEN` value using the [Long-Lived Access Tokens](https://developers.home-assistant.io/docs/auth_api/#long-lived-access-token) section at the bottom of your [Home Assistant Profile page](https://www.home-assistant.io/docs/authentication/#your-account-profile).

`PLATES_URL` is optional. If set, it must point to a **publicly accessible** URL that conatins images of plates.

## Potential Future Improvements

- Android notification support.
- Better logbook entries.
- MMC (Make, Model, Color) support.
- Allow muting notifications for a particular plate.