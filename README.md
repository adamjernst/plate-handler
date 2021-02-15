# Plate Handler

Links [Plate Recognizer](https://platerecognizer.com)'s [Stream API](https://platerecognizer.com/stream/) to [Home Assistant](https://www.home-assistant.io).

<img src="img/notif.jpeg" width="250" alt="Screenshot of a push notification">

- Receive [notifications](https://companion.home-assistant.io/docs/notifications/notifications-basic) when plates are spotted.
- Associate names with plates.
- Automatic [logbook](https://www.home-assistant.io/integrations/logbook/) entries and [events](https://www.home-assistant.io/docs/configuration/events/) for spotted plates.

## Setup

If you're using [Docker Compose](https://docs.docker.com/compose/), set up plate-handler as a service:

```
version: 3.1
services:
  plate-handler:
    container_name: plate-handler
    image: adamjernst/plate-handler:latest
    restart: unless-stopped
    # plate-handler connects to Home Assistant on port 8123;
    # plate-recognizer connects to plate-handler via webhook
    # on port 8402. Host networking is the simplest way to
    # allow this, but custom networks would be more secure.
    network_mode: "host"
    volumes:
      - /var/plate-handler:/data
      - /home/username/.homeassistant/www/plates:/plates
    environment:
      ACCESS_TOKEN: "generate-in-home-assistant"
      NOTIFY_DEVICE: mobile_app_bobs_iphone
      PLATES_URL: "https://yoursite.ui.nabu.casa/local/plates/"
```

Generate the `ACCESS_TOKEN` value using the [Long-Lived Access Tokens](https://developers.home-assistant.io/docs/auth_api/#long-lived-access-token) section at the bottom of your [Home Assistant Profile page](https://www.home-assistant.io/docs/authentication/#your-account-profile).

`NOTIFY_DEVICE` should be set to `mobile_app_` followed by the name of your mobile device, as described in the Home Assistant documentation:

> [Your device name is found] in the App Configuration menu of the [Home Assistant app] sidebar and defaults to the name specified in the General>About within the iOS Settings app or under About>Phone in Android settings (**with spaces and non-alphanumeric characters replaced by underscores**).

For example, if your iPhone is called "Bob's iPhone", `NOTIFY_DEVICE` should be set to `mobile_app_bobs_iphone`.

`PLATES_URL` is optional. If set, it must point to a **publicly accessible** URL that conatins images of plates. See "Images" below.

`HOST` can optionally be set to the hostname and port for connecting the Home Assistant API. If unset, it defaults to `localhost:8123`.

## Configuring Plate Recognizer Stream

You must use the `webhook_target` setting in Plate Recognizer Stream's `config.ini` file. By default the webhook listens on port 8402 at the path `/webhook/`. An example configuration:

```
[cameras]
  [[camera-1]]
    active = yes
    url = rtsp://user:pass@192.168.1.2:554/
    name = LPR

    webhook_target = http://localhost:8402/webhook/
    webhook_image = yes
    webhook_image_type = original
```

## Volumes

**`/data`** is used to store a SQLite database with names for plates and the times those plates were spotted. This can be mapped to any path on the host machine. The example Compose configuration above shows a [bind mount](https://docs.docker.com/storage/bind-mounts/) to `/var/plate-handler`.

**`/plates`** is used to store images of plates, if the `PLATES_URL` environment variable is set. See "Images" below.

## Actionable Notifications

Enable [actionable notifictions](https://companion.home-assistant.io/docs/notifications/actionable-notifications/) to allow associating names with license plates, so that future notifications for the same plate will reference the name.

For example, instead of "Spotted plate ABC5555", notifications will read "Spotted plate for Emma."

To enable actionable notifications, place the following in Home Assistant's [`configuration.yaml`](https://www.home-assistant.io/docs/configuration/):

```
ios:
  push:
    categories:
      - name: Plate
        identifier: 'plate'
        actions:
          - identifier: 'NAME'
            title: 'Save Name...'
            behavior: 'textInput'
            textInputButtonTitle: 'Save'
            textInputPlaceholder: 'e.g. "John" or "Trash Pickup"'
```

After editing `configuration.yaml`, restart Home Assistant via Configuration &rarr; Server Controls &rarr; Restart; then sync push categories in your iOS app by opening App Configuration &rarr; Notifications.

## Images

Home Assistant has support for [hosting files](https://www.home-assistant.io/integrations/http/#hosting-files), which can be used to include an image of the license plate with notifications. To enable image notifications:

**Create a `www/plates` subdirectory in your Home Assistant config directory**. If you are running Home Assistant in a Docker container, this is probably `~/.homeassistant/`, so:

```
# mkdir -p ~/.homeassistant/www/plates
```

**Map `/plates` to the `www/plates` directory.** This is already done in the sample Docker Compose configuration above:

```
...
    volumes:
      - /home/username/.homeassistant/www/plates:/plates
```

**Set the `PLATES_URL` environment variable to a publicly accessible URL representing the plates directory.** This is also already done in the sample Docker Compose configuration above, but you must substitute the URL for your site. Assuming you use [Home Assistant Cloud](https://www.nabucasa.com), the URL will look something like this:

```
    environment:
      PLATES_URL: "https://yoursite.ui.nabu.casa/local/plates/"
```

## Device Groups

To send notifications to more than one device, create a [notification group](https://companion.home-assistant.io/docs/notifications/notifications-basic#sending-notifications-to-multiple-devices).

For example, add the following to your Home Assistant [`configuration.yaml`](https://www.home-assistant.io/docs/configuration/) file to create a group called `ALL_DEVICES`:

```
notify:
  - name: ALL_DEVICES
    platform: group
    services:
      - service: mobile_app_bobs_iphone
      - service: mobile_app_lindas_iphone
```

After editing `configuration.yaml`, restart Home Assistant via Configuration &rarr; Server Controls &rarr; Restart.

Set the `NOTIFY_DEVICE` environment variable to the name of your group, e.g. `ALL_DEVICES`.

## Potential Future Improvements

- Create a [Home Assistant Add-on](https://www.home-assistant.io/addons/) to streamline setup.
- Android notification support.
- Better logbook entries.
- MMC (Make, Model, Color) support.
- Allow muting notifications for a particular plate.