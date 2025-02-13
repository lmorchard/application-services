/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]
//! # Rust Push Component
//!
//! This component helps an application to manage [WebPush](https://developer.mozilla.org/en-US/docs/Web/API/Push_API) subscriptions,
//! acting as an intermediary between Mozilla's [autopush service](https://autopush.readthedocs.io/en/latest/)
//! and platform native push infrastructure such as [Firebase Cloud Messaging](https://firebase.google.com/docs/cloud-messaging) or [Amazon Device Messaging](https://developer.amazon.com/docs/adm/overview.html).
//!
//! ## Background Concepts
//!
//! ### WebPush Subscriptions
//!
//! A WebPush client manages a number of *subscriptions*, each of which is used to deliver push
//! notifications to a different part of the app. For example, a web browser might manage a separate
//! subscription for each website that has registered a [service worker](https://developer.mozilla.org/en-US/docs/Web/API/Service_Worker_API), and an application that includes Firefox Accounts would manage
//! a dedicated subscription on which to receive account state updates.
//!
//! Each subscription is identified by a unique *channel id*, which is a randomly-generated identifier.
//! It's the responsibility of the application to know how to map a channel id to an appropriate function
//! in the app to receive push notifications. Subscriptions also have an associated *scope* which is something
//! to do which service workers that your humble author doesn't really understand :-/.
//!
//! When a subscription is created for a channel id, we allocate *subscription info* consisting of:
//!
//! * An HTTP endpoint URL at which push messages can be submitted.
//! * A cryptographic key and authentication secret with which push messages can be encrypted.
//!
//! This subscription info is distributed to other services that want to send push messages to
//! the application.
//!
//! The HTTP endpoint is provided by Mozilla's [autopush service](https://autopush.readthedocs.io/en/latest/),
//! and we use the [rust-ece](https://github.com/mozilla/rust-ece) to manage encryption with the cryptographic keys.
//!
//! Here's a helpful diagram of how the *subscription* flow works at a high level across the moving parts:
//! ![A Sequence diagram showing how the different parts of push interact](https://mozilla.github.io/application-services/book/diagrams/Push-Component-Subscription-flow.png "Sequence diagram")
//!
//! ### AutoPush Bridging
//!
//! Our target consumer platforms each have their own proprietary push-notification infrastructure,
//! such as [Firebase Cloud Messaging](https://firebase.google.com/docs/cloud-messaging) for Android
//! and the [Apple Push Notification Service](https://developer.apple.com/notifications/) for iOS.
//! Mozilla's [autopush service](https://autopush.readthedocs.io/en/latest/) provides a bridge between
//! these different mechanisms and the WebPush standard so that they can be used with a consistent
//! interface.
//!
//! This component acts a client of the [Push Service Bridge HTTP Interface](https://autopush.readthedocs.io/en/latest/http.html#push-service-bridge-http-interface).
//!
//! We assume two things about the consuming application:
//! * It has registered with the autopush service and received a unique `app_id` identifying this registration.
//! * It has registred with whatever platform-specific notification infrastructure is appropriate, and is
//!   able to obtain a `token` corresponding to its native push notification state.
//!
//! On first use, this component will register itself as an *application instance* with the autopush service, providing the `app_id` and `token` and receiving a unique `uaid` ("user-agent id") to identify its
//! connection to the server.
//!
//! As the application adds or removes subscriptions using the API of this component, it will:
//! * Manage a local database of subscriptions and the corresponding cryptographic material.
//! * Make corresponding HTTP API calls to update the state associated with its `uaid` on the autopush server.
//!
//! Periodically, the application should call a special `verify_connection` method to check whether
//! the state on the autopush server matches the local state and take any corrective action if it
//! differs.
//!
//! For local development and debugging, it is possible to run a local instance of the autopush
//! bridge service; see [this google doc](https://docs.google.com/document/d/18L_g2hIj_1mncF978A_SHXN4udDQLut5P_ZHYZEwGP8) for details.
//!
//! ## API
//!
//! ## Initialization
//!
//! Calls are handled by the `PushManager`, which provides a handle for future calls.
//!
//! example:
//! ```kotlin
//!
//! import mozilla.appservices.push.(PushManager, BridgeTypes)
//!
//! // The following are mock calls for fetching application level configuration options.
//! // "SenderID" is the native OS push message application identifier. See Native
//! // messaging documentation for details.
//! val sender_id = SystemConfigurationOptions.get("SenderID")
//!
//! // The "bridge type" is the identifier for the native OS push message system.
//! // (e.g. FCM for Google Firebase Cloud Messaging, ADM for Amazon Direct Messaging,
//! // etc.)
//! val bridge_type = BridgeTypes.FCM
//!
//! // The "registration_id" is the native OS push message user registration number.
//! // Native push message registration usually happens at application start, and returns
//! // an opaque user identifier string. See Native messaging documentation for details.
//! val registration_id = NativeMessagingSystem.register(sender_id)
//!
//! val push_manager = PushManager(
//!     sender_id,
//!     bridge_type,
//!     registration_id
//! )
//!
//! // It is strongly encouraged that the connection is verified at least once a day.
//! // This will ensure that the server and UA have matching information regarding
//! // subscriptions. This call usually returns quickly, but may take longer if the
//! // UA has a large number of subscriptions and things have fallen out of sync.
//!
//! for change in push_manager.verify_connection() {
//!     // fetch the subscriber from storage using the change[0] and
//!     // notify them with a `pushsubscriptionchange` message containing the new
//!     // endpoint change[1]
//! }
//!
//! ```
//!
//! ## New subscription
//!
//! Before messages can be delivered, a new subscription must be requested. The subscription info block contains all the information a remote subscription provider service will need to encrypt and transmit a message to this user agent.
//!
//! example:
//! ```kotlin
//!
//! // Each new request must have a unique "channel" identifier. This channel helps
//! // later identify recipients and aid in routing. A ChannelID is a UUID4 value.
//! // the "scope" is the ServiceWorkerRegistration scope. This will be used
//! // later for push notification management.
//! val channelID = GUID.randomUUID()
//!
//! val subscription_info = push_manager.subscribe(channelID, endpoint_scope)
//!
//! // the published subscription info has the following JSON format:
//! // {"endpoint": subscription_info.endpoint,
//! //  "keys": {
//! //      "auth": subscription_info.keys.auth,
//! //      "p256dh": subscription_info.keys.p256dh
//! //  }}
//! ```
//!
//! ## End a subscription
//!
//! A user may decide to no longer receive a given subscription. To remove a given subscription, pass the associated channelID
//!
//! ```kotlin
//! push_manager.unsubscribe(channelID)  // Terminate a single subscription
//! ```
//!
//! If the user wishes to terminate all subscriptions, send and empty string for channelID
//!
//! ```kotlin
//! push_manager.unsubscribe("")        // Terminate all subscriptions for a user
//! ```
//!
//! If this function returns `false` the subsequent `verify_connection` may result in new channel endpoints.
//!
//! ## Decrypt an incoming subscription message
//!
//! An incoming subscription body will contain a number of metadata elements along with the body of the message. Due to platform differences, how that metadata is provided may //! vary, however the most common form is that the messages "payload" looks like.
//!
//! ```javascript
//! {"chid": "...",         // ChannelID
//!  "con": "...",          // Encoding form
//!  "enc": "...",          // Optional encryption header
//!  "crypto-key": "...",   // Optional crypto key header
//!  "body": "...",         // Encrypted message body
//! }
//! ```
//! These fields may be included as a sub-hash, or may be intermingled with other data fields. If you have doubts or concerns, please contact the Application Services team guidance
//!
//! Based on the above payload, an example call might look like:
//!
//! ```kotlin
//!     val result = manager.decrypt(
//!         channelID = payload["chid"].toString(),
//!         body = payload["body"].toString(),
//!         encoding = payload["con"].toString(),
//!         salt = payload.getOrElse("enc", "").toString(),
//!         dh = payload.getOrElse("dh", "").toString()
//!     )
//!     // result returns a byte array. You may need to convert to a string
//!     return result.toString(Charset.forName("UTF-8"))
//!```

uniffi_macros::include_scaffolding!("push");
// All implementation detail lives in the `internal` module
mod internal;
use std::sync::Mutex;

pub use crate::internal::error::*;

// The following are only exposed for use by the examples
pub use internal::communications::Connection;
pub use internal::crypto::get_random_bytes;
pub use internal::error::Result as InternalResult;
pub use internal::storage::Storage as InternalStorage;
pub use internal::PushConfiguration;
pub use internal::PushManager as InternalPushManager;
// =====================

/// Object representing the PushManager used to manage subscriptions
///
/// The `PushManager` object is the main interface provided by this crate
/// it allow consumers to manage push subscriptions. It exposes methods that
/// interact with the [`autopush server`](https://autopush.readthedocs.io/en/latest/)
/// and persists state representing subscriptions.
pub struct PushManager {
    // We serialize all access on a mutex for thread safety
    // TODO: this can improved by making the locking more granular
    // and moving the mutex down to ensure `internal::PushManager`
    // is Sync + Send
    internal: Mutex<internal::PushManager>,
}

impl PushManager {
    /// Creates a new [`PushManager`] object, not subscribed to any
    /// channels
    ///
    /// # Arguments
    ///
    ///   - `sender_id` - Sender/Application ID value
    ///   - `server_host` - The host name for the service (e.g. "updates.push.services.mozilla.com").
    ///   - `http_protocol` - The optional socket protocol (default: "https")
    ///   - `bridge_type` - The [`BridgeType`] the consumer would like to use to deliver the push messages
    ///   - `registration_id` - The native OS messaging registration ID
    ///   - `database_path` - The path where [`PushManager`] will store persisted state
    ///
    /// # Errors
    /// Returns an error in the following cases:
    ///   - PushManager is unable to open the `database_path` given
    ///   - PushManager is unable to establish a connection to the autopush server
    pub fn new(
        sender_id: String,
        server_host: String,
        http_protocol: String,
        bridge_type: BridgeType,
        registration_id: String,
        database_path: String,
    ) -> Result<Self> {
        let bridge_type = match bridge_type {
            BridgeType::Adm => "adm",
            BridgeType::Apns => "apns",
            BridgeType::Fcm => "fcm",
            BridgeType::Test => "test",
        }
        .to_string();
        if !registration_id.is_empty() {
            log::warn!("`registration_id` is ignored/deprecated when creating a push manager.");
        }
        // XXX - we probably should persist, say, this as JSON and ensure it's the same
        // on each run, then nuke the DB if not. Eg, imagine "bridge_type" changing, things
        // would break badly. Unlikely, so later...
        let config = PushConfiguration {
            server_host,
            http_protocol: Some(http_protocol),
            bridge_type: Some(bridge_type),
            sender_id,
            database_path: Some(database_path),
            ..Default::default()
        };
        Ok(Self {
            internal: Mutex::new(internal::PushManager::new(config)?),
        })
    }

    /// Subscribes to a new channel and gets the Subscription Info block
    ///
    /// # Arguments
    ///   - `channel_id` - Channel ID (UUID4) for new subscription, either pre-generated or "" and one will be created.
    ///   - `scope` - Site scope string (defaults to "" for no site scope string).
    ///   - `server_key` - optional VAPID public key to "lock" subscriptions (defaults to "" for no key)
    ///
    /// # Returns
    /// A Subscription response that includes the following:
    ///   - A URL that can be used to deliver push messages
    ///   - A cryptographic key that can be used to encrypt messages
    ///     that would then be decrypted using the [`PushManager::decrypt`] function
    ///
    /// # Errors
    /// Returns an error in the following cases:
    ///   - PushManager was unable to access its persisted storage
    ///   - An error occurred sending a subscription request to the autopush server
    ///   - An error occurred generating or deserializing the cryptographic keys
    pub fn subscribe(
        &self,
        channel_id: &str,
        scope: &str,
        server_key: &Option<String>,
    ) -> Result<SubscriptionResponse> {
        self.internal
            .lock()
            .unwrap()
            .subscribe(channel_id, scope, server_key.as_deref())
    }

    /// Unsubscribe from given channelID, ending that subscription for the user.
    ///
    /// # Arguments
    ///   - `channel_id` - Channel ID (UUID) for subscription to remove
    ///
    /// # Returns
    /// Returns a boolean indicating if un-subscription was successful
    ///
    /// # Errors
    /// Returns an error in the following cases:
    ///   - The PushManager does not contain a valid UAID
    ///   - An error occurred sending an unsubscribe request to the autopush server
    ///   - An error occurred accessing the PushManager's persisted storage
    pub fn unsubscribe(&self, channel_id: &str) -> Result<bool> {
        self.internal.lock().unwrap().unsubscribe(channel_id)
    }

    /// Unsubscribe all channels for the user
    ///
    /// # Errors
    /// Returns an error in the following cases:
    ///   - The PushManager does not contain a valid UAID
    ///   - An error occurred sending an unsubscribe request to the autopush server
    ///   - An error occurred accessing the PushManager's persisted storage
    pub fn unsubscribe_all(&self) -> Result<()> {
        self.internal.lock().unwrap().unsubscribe_all()
    }

    /// Updates the Native OS push registration ID.
    /// **NOTE**: If this returns false, the subsequent [`PushManager::verify_connection`]
    /// may result in new endpoint registration
    ///
    /// # Arguments:
    ///   - `new_token` - the new Native OS push registration ID
    ///
    /// # Returns
    /// Returns a boolean indicating if the update was successful
    ///
    /// # Errors
    /// Return an error in the following cases:
    ///   - The PushManager does not contain a valid UAID
    ///   - An error occurred sending an update request to the autopush server
    ///   - An error occurred accessing the PushManager's persisted storage
    pub fn update(&self, new_token: &str) -> Result<bool> {
        self.internal.lock().unwrap().update(new_token)
    }

    /// Verifies the connection state
    ///
    /// **NOTE**: This does not resubscribe to any channels
    /// it only returns the list of channels that the client should
    /// re-subscribe to.
    ///
    /// # Returns
    /// Returns a list of [`PushSubscriptionChanged`]
    /// indicating the channels the consumer the client should re-subscribe
    /// to. If the list is empty, the client's connection was verified
    /// successfully, and the client does not need to resubscribe
    ///
    /// # Errors
    /// Return an error in the following cases:
    ///   - The PushManager does not contain a valid UAID
    ///   - An error occurred sending an channel list retrieval request to the autopush server
    ///   - An error occurred accessing the PushManager's persisted storage
    pub fn verify_connection(&self) -> Result<Vec<PushSubscriptionChanged>> {
        self.internal.lock().unwrap().verify_connection()
    }

    /// Decrypts a raw push message.
    ///
    /// This accepts the content of a Push Message (from websocket or via Native Push systems).
    /// # Arguments:
    ///   - `channel_id` - the ChannelID (included in the envelope of the message)
    ///   - `body` - The encrypted body of the message
    ///   - `encoding` - The Content Encoding "enc" field of the message (defaults to "aes128gcm")
    ///   - `salt` - The "salt" field (if present in the raw message, defaults to "")
    ///   - `dh` - The "dh" field (if present in the raw message, defaults to "")
    ///
    /// # Returns
    /// Decrypted message body as a signed byte array
    /// they byte array is signed to allow consumers (Kotlin only at the time of this documentation)
    /// to work easily with the message. (They can directly call `.toByteArray` on it)
    ///
    /// # Errors
    /// Returns an error in the following cases:
    ///   - The PushManager does not contain a valid UAID
    ///   - There are no records associated with the UAID the [`PushManager`] contains
    ///   - An error occurred while decrypting the message
    ///   - An error occurred accessing the PushManager's persisted storage
    pub fn decrypt(
        &self,
        channel_id: &str,
        body: &str,
        encoding: &str,
        salt: &str,
        dh: &str,
    ) -> Result<Vec<i8>> {
        let decrypted = self.internal.lock().unwrap().decrypt(
            channel_id,
            body,
            encoding,
            Some(salt),
            Some(dh),
        )?;

        // NOTE: this returns a `Vec<i8>` since the kotlin consumer is expecting
        // signed bytes.
        Ok(decrypted.into_iter().map(|ub| ub as i8).collect())
    }

    /// Get the dispatch info for a given subscription channel
    ///
    /// # Arguments
    ///   - `channel_id`: The subscription channelID
    ///
    /// # Returns
    /// [`DispatchInfo`] containing the channel ID and scope string
    ///
    /// # Errors
    /// Returns an error in the following cases:
    ///   - An error occurred accessing the persisted storage
    pub fn dispatch_info_for_chid(&self, channel_id: &str) -> Result<Option<DispatchInfo>> {
        self.internal.lock().unwrap().get_record_by_chid(channel_id)
    }
}

/// Public facing Error that the crate produces
///
/// This is created from an internal error as the error passes through the FFI

#[derive(Debug, thiserror::Error)]
pub enum PushError {
    /// An unspecified general error has occured
    #[error("General Error: {0:?}")]
    GeneralError(String),

    /// An error occurred while running a cryptographic operation
    #[error("Crypto error: {0}")]
    CryptoError(String),

    /// A Client communication error
    #[error("Communication Error: {0:?}")]
    CommunicationError(String),

    /// An error returned from the registration Server
    #[error("Communication Server Error: {0}")]
    CommunicationServerError(String),

    /// Channel is already registered, generate new channelID
    #[error("Channel already registered.")]
    AlreadyRegisteredError,

    /// An error with Storage
    #[error("Storage Error: {0:?}")]
    StorageError(String),

    #[error("No record for chid {0:?}")]
    RecordNotFoundError(String),

    /// A failure to encode data to/from storage.
    #[error("Error executing SQL: {0}")]
    StorageSqlError(String),

    /// The registration token could not be found
    #[error("Missing Registration Token")]
    MissingRegistrationTokenError,

    #[error("Transcoding Error: {0}")]
    TranscodingError(String),

    /// A failure to parse a URL.
    #[error("URL parse error: {0:?}")]
    UrlParseError(String),

    /// A failure deserializing json.
    #[error("Failed to parse json: {0}")]
    JSONDeserializeError(String),

    /// The UAID was not recognized by the server
    #[error("Unrecognized UAID: {0}")]
    UAIDNotRecognizedError(String),

    /// Was unable to send request to server
    #[error("Unable to send request to server: {0}")]
    RequestError(String),

    #[error("Error opening database: {0}")]
    OpenDatabaseError(String),
}

/// The types of supported native bridges.
///
/// FCM = Google Android Firebase Cloud Messaging
/// ADM = Amazon Device Messaging for FireTV
/// APNS = Apple Push Notification System for iOS
///
/// Please contact services back-end for any additional bridge protocols.
///
pub enum BridgeType {
    Fcm,
    Adm,
    Apns,
    Test,
}

// We define how to convert from an internal error
// into the external facing [`PushError`]
// note that the some variants of the internal error
// carry another error they were generated from
// this information is dropped and replaced with a message
// which is the stringified error
impl From<internal::error::Error> for PushError {
    fn from(err: internal::error::Error) -> Self {
        match err.kind() {
            ErrorKind::GeneralError(message) => PushError::GeneralError(message.clone()),
            ErrorKind::CryptoError(message) => PushError::CryptoError(message.clone()),
            ErrorKind::CommunicationError(message) => {
                PushError::CommunicationError(message.clone())
            }
            ErrorKind::CommunicationServerError(message) => {
                PushError::CommunicationServerError(message.clone())
            }
            ErrorKind::AlreadyRegisteredError => PushError::AlreadyRegisteredError,
            ErrorKind::StorageError(message) => PushError::StorageError(message.clone()),
            ErrorKind::RecordNotFoundError(chid) => PushError::RecordNotFoundError(chid.clone()),
            ErrorKind::StorageSqlError(e) => PushError::StorageSqlError(e.to_string()),
            ErrorKind::MissingRegistrationTokenError => PushError::MissingRegistrationTokenError,
            ErrorKind::TranscodingError(message) => PushError::TranscodingError(message.clone()),
            ErrorKind::UrlParseError(e) => PushError::UrlParseError(e.to_string()),
            ErrorKind::JSONDeserializeError(e) => PushError::JSONDeserializeError(e.to_string()),
            ErrorKind::UAIDNotRecognizedError(message) => {
                PushError::UAIDNotRecognizedError(message.clone())
            }
            ErrorKind::RequestError(e) => PushError::RequestError(e.to_string()),
            ErrorKind::OpenDatabaseError(e) => PushError::OpenDatabaseError(e.to_string()),
        }
    }
}

/// Dispatch Information returned from [`PushManager::dispatch_info_for_chid`]
#[derive(Debug, Clone, PartialEq)]
pub struct DispatchInfo {
    pub scope: String,
    pub endpoint: String,
    pub app_server_key: Option<String>,
}

/// Key Information that can be used to encrypt payloads
#[derive(Debug, Clone, PartialEq)]
pub struct KeyInfo {
    pub auth: String,
    pub p256dh: String,
}
/// Subscription Information, the endpoint to send push messages to and
/// the key information that can be used to encrypt payloads
#[derive(Debug, Clone, PartialEq)]
pub struct SubscriptionInfo {
    pub endpoint: String,
    pub keys: KeyInfo,
}

/// The subscription response object returned from [`PushManager::subscribe`]
#[derive(Debug, Clone, PartialEq)]
pub struct SubscriptionResponse {
    pub channel_id: String,
    pub subscription_info: SubscriptionInfo,
}

/// An dictionary describing the push subscription that changed, the caller
/// will receive a list of [`PushSubscriptionChanged`] when calling
/// [`PushManager::verify_connection`], one entry for each channel that the
/// caller should resubscribe to
#[derive(Debug, Clone, PartialEq)]
pub struct PushSubscriptionChanged {
    pub channel_id: String,
    pub scope: String,
}
