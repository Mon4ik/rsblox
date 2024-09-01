use reqwest::header;
use serde::{Deserialize, Serialize};

use super::{Limit, RobloxApi, RobloxError};

mod request_types;

const ROBUX_API_PART_1: &str = "https://economy.roblox.com/v1/users/";
const ROBUX_API_PART_2: &str = "/currency";

const RESELLERS_API_PART_1: &str = "https://economy.roblox.com/v1/assets/";
const RESELLERS_API_PART_2: &str = "/resellers";

const TRANSACTIONS_API_PART_1: &str = "https://economy.roblox.com/v2/users/";
const TRANSACTIONS_API_PART_2: &str = "/transactions";

const TOGGLE_SALE_API_PART_1: &str = "https://economy.roblox.com/v1/assets/";
const TOGGLE_SALE_API_PART_2: &str = "/resellable-copies/";

const USER_SALES_TRANSACTION_TYPE: &str = "Sale";

/// Custom Roblox errors that occur when using [`Client::purchase_tradable_limited`].
#[non_exhaustive]
#[derive(
    thiserror::Error,
    Debug,
    Default,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
pub enum PurchaseTradableLimitedError {
    /// Thrown when the user has a pending transaction.
    /// However, Roblox will also throw this when it doesn't know what error to give.
    /// If you are trying to keep buying a limited item, ignore this error and try again until
    /// [`PurchaseTradableLimitedError::ItemNotForSale`] is thrown.
    #[default]
    #[error("Pending Transaction.")]
    PendingTransaction,
    /// Thrown when the user tries to buy a limited item that is not for sale.
    /// There is no point in retrying after this error.
    #[error("Item Not For Sale.")]
    ItemNotForSale,
    /// Thrown when the user does not have enough robux to buy the item.
    /// There is no point in retrying after this error.
    #[error("Not Enough Robux.")]
    NotEnoughRobux,
    /// Thrown when the user tries to buy an item for an incorrect price (or the seller
    /// somehow changed the price really fast). If this error is thrown, I would keep trying to
    /// buy the item until [`PurchaseTradableLimitedError::ItemNotForSale`] is thrown.
    #[error("Price Changed")]
    PriceChanged,
    /// Thrown when the user tries to buy their own item. There is no point in retrying after.
    #[error("Cannot Buy Own Item")]
    CannotBuyOwnItem,
    /// Thrown when an unknown error occurs. If this error is thrown, I would keep
    /// trying to buy the item until [`PurchaseTradableLimitedError::ItemNotForSale`] is thrown.
    #[error("Unknown Roblox Error Message: {0}")]
    UnknownRobloxErrorMsg(String),
}

// todo: change this to User maybe
/// A reseller of a resale listing.
#[allow(missing_docs)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct Reseller {
    pub user_id: u64,
    pub name: String,
}

/// A resale listing of a limited item.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct Listing {
    /// The unique asset id of the item.
    pub uaid: u64,
    /// The price of the listing.
    pub price: u64,
    /// The reseller of the listing.
    pub reseller: Reseller,
    /// The serial number of the item. This is separate from the uaid and only
    /// exists for Limited U Items.
    pub serial_number: Option<u64>,
}

/// A sale of an asset from the user's transaction history. Retrieved from <https://economy.roblox.com/v2/users/{user_id}/transactions?transactionType=Sale>.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct UserSale {
    /// These appear to be generated in sequential order and appear to be
    /// only related to Sales.
    pub sale_id: u64,
    /// Whether the sale is still pending
    pub is_pending: bool,
    /// The id if the user that purchased the asset.
    pub user_id: u64,
    /// The display name of the user that purchased the asset.
    pub user_display_name: String,
    /// The robux the user received after tax. Note that it's not certain that every
    /// type of item has a 30% tax, so the value is left as-is. To convert this to a price
    /// that the item sold at (assuming 30% tax), use `robux_received * 1.428`.
    pub robux_received: u64,
    /// The asset id of the item that was sold.
    pub asset_id: u64,
    /// The name of the asset that was sold.
    pub asset_name: String,
}

impl RobloxApi {
    /// Grabs robux count of the current account from <https://economy.roblox.com/v1/users/{user_id}/currency>.
    ///
    /// # Notes
    /// * Requires a valid roblosecurity.
    ///
    /// # Errors
    /// * All errors under [Standard Errors](#standard-errors).
    /// * All errors under [Auth Required Errors](#auth-required-errors).
    ///
    /// # Example
    /// ```no_run
    /// use roboat::ClientBuilder;
    ///
    /// const ROBLOSECURITY: &str = "roblosecurity";
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ClientBuilder::new().roblosecurity(ROBLOSECURITY.to_string()).build();
    ///
    /// let robux = client.robux().await?;
    /// println!("Robux: {}", robux);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn robux(&self) -> Result<u64, RobloxError> {
        let user_id = self.user_id().await?;
        let formatted_url = format!("{}{}{}", ROBUX_API_PART_1, user_id, ROBUX_API_PART_2);
        let cookie = self.cookie_string().await?;

        let request_result = self
            .reqwest_client
            .get(formatted_url)
            .header(header::COOKIE, cookie)
            .send()
            .await;

        let response = Self::validate_request_result(request_result).await?;
        let raw = Self::parse_to_raw::<request_types::CurrencyResponse>(response).await?;

        let robux = raw.robux;

        Ok(robux)
    }

    /// Grabs resellers of an item from <https://economy.roblox.com/v1/assets/{item_id}/resellers?cursor={cursor}&limit={limit}>.
    ///
    /// # Notes
    /// * Requires a valid roblosecurity.
    ///
    /// # Argument Notes
    /// * The cursor is used to get the a certain page of results. If you want the starting page, use `None`.
    ///
    /// # Return Value Notes
    /// * The first value is a vector of reseller listings.
    /// * The second value is the cursor for the next page of results. If there are no more pages, this will be `None`.
    ///
    /// # Errors
    /// * All errors under [Standard Errors](#standard-errors).
    /// * All errors under [Auth Required Errors](#auth-required-errors).
    ///
    /// # Example
    /// ```no_run
    /// use roboat::Limit;
    /// use roboat::ClientBuilder;
    ///
    /// const ROBLOSECURITY: &str = "roblosecurity";
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ClientBuilder::new().roblosecurity(ROBLOSECURITY.to_string()).build();
    ///
    /// let item_id = 1365767;
    /// let limit = Limit::Ten;
    /// let cursor = None;
    ///
    /// let (resellers, next_page_cursor) = client.resellers(item_id, limit, cursor).await?;
    /// println!("Lowest Price for Item {}: {}", item_id, resellers[0].price);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn resellers(
        &self,
        item_id: u64,
        limit: Limit,
        cursor: Option<String>,
    ) -> Result<(Vec<Listing>, Option<String>), RobloxError> {
        let limit = limit.to_u64();
        let cursor = cursor.unwrap_or_default();
        let cookie = self.cookie_string().await?;

        let formatted_url = format!(
            "{}{}{}?cursor={}&limit={}",
            RESELLERS_API_PART_1, item_id, RESELLERS_API_PART_2, cursor, limit
        );

        let request_result = self
            .reqwest_client
            .get(formatted_url)
            .header(header::COOKIE, cookie)
            .send()
            .await;

        let response = Self::validate_request_result(request_result).await?;
        let raw = Self::parse_to_raw::<request_types::ResellersResponse>(response).await?;

        let next_page_cursor = raw.next_page_cursor;

        let mut listings = Vec::new();

        for listing in raw.data {
            let reseller = Reseller {
                user_id: listing.seller.id,
                name: listing.seller.name,
            };

            let listing = Listing {
                uaid: listing.user_asset_id,
                price: listing.price,
                reseller,
                serial_number: listing.serial_number,
            };

            listings.push(listing);
        }

        Ok((listings, next_page_cursor))
    }

    /// Grabs user sales from <https://economy.roblox.com/v2/users/{user_id}/transactions?transactionType=Sale&cursor={cursor}&limit={limit}>.
    ///
    /// # Notes
    /// * Requires a valid roblosecurity.
    ///
    /// # Argument Notes
    /// * The cursor is used to get the a certain page of results. If you want the starting page, use `None`.
    ///
    /// # Return Value Notes
    /// * The first value is a vector of user sales.
    /// * The second value is the cursor for the next page of results. If there are no more pages, this will be `None`.
    ///
    /// # Errors
    /// * All errors under [Standard Errors](#standard-errors).
    /// * All errors under [Auth Required Errors](#auth-required-errors).
    ///
    /// # Example
    /// ```no_run
    /// use roboat::Limit;
    /// use roboat::ClientBuilder;
    ///
    /// const ROBLOSECURITY: &str = "roblosecurity";
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ClientBuilder::new().roblosecurity(ROBLOSECURITY.to_string()).build();
    ///
    /// let limit = Limit::Ten;
    /// let cursor = None;
    ///
    /// let (user_sales, next_page_cursor) = client.user_sales(limit, cursor).await?;
    ///
    /// let sale_amount = user_sales.len();
    /// let total_robux_earned = user_sales
    ///     .iter()
    ///     .map(|sale| sale.robux_received)
    ///     .sum::<u64>();
    ///
    /// println!("Robux gained from last {} sales: {}", sale_amount, total_robux_earned);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn user_sales(
        &self,
        limit: Limit,
        cursor: Option<String>,
    ) -> Result<(Vec<UserSale>, Option<String>), RobloxError> {
        let limit = limit.to_u64();
        let cursor = cursor.unwrap_or_default();

        let user_id = self.user_id().await?;

        let formatted_url = format!(
            "{}{}{}?cursor={}&limit={}&transactionType={}",
            TRANSACTIONS_API_PART_1,
            user_id,
            TRANSACTIONS_API_PART_2,
            cursor,
            limit,
            USER_SALES_TRANSACTION_TYPE
        );

        let cookie = self.cookie_string().await?;

        let request_result = self
            .reqwest_client
            .get(formatted_url)
            .header(header::COOKIE, cookie)
            .send()
            .await;

        let response = Self::validate_request_result(request_result).await?;
        let raw = Self::parse_to_raw::<request_types::UserSalesResponse>(response).await?;

        let next_page_cursor = raw.next_page_cursor;

        let mut sales = Vec::new();

        for raw_sale in raw.data {
            let sale_id = raw_sale.id;
            let asset_id = raw_sale.details.id;
            let robux_received = raw_sale.currency.amount;
            let is_pending = raw_sale.is_pending;
            let user_id = raw_sale.agent.id;
            let user_display_name = raw_sale.agent.name;
            let asset_name = raw_sale.details.name;

            let sale = UserSale {
                sale_id,
                asset_id,
                robux_received,
                is_pending,
                user_id,
                user_display_name,
                asset_name,
            };

            sales.push(sale);
        }

        Ok((sales, next_page_cursor))
    }

    /// Puts a limited item on sale using the endpoint <https://economy.roblox.com/v1/assets/{item_id}/resellable-copies/{uaid}>.
    ///
    /// # Notes
    /// * Requires a valid roblosecurity.
    /// * Will repeat once if the x-csrf-token is invalid.
    ///
    /// # Return Value Notes
    /// * Will return `Ok(())` if the item was successfully put on sale.
    ///
    /// # Errors
    /// * All errors under [Standard Errors](#standard-errors).
    /// * All errors under [Auth Required Errors](#auth-required-errors).
    /// * All errors under [X-CSRF-TOKEN Required Errors](#x-csrf-token-required-errors).
    ///
    /// # Example
    /// ```no_run
    /// use roboat::ClientBuilder;
    ///
    /// const ROBLOSECURITY: &str = "roblosecurity";
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ClientBuilder::new().roblosecurity(ROBLOSECURITY.to_string()).build();
    ///
    /// let item_id = 123456789;
    /// let uaid = 987654321;
    /// let price = 5000;
    ///
    /// match client.put_limited_on_sale(item_id, uaid, price).await {
    ///    Ok(_) => println!("Successfully put item on sale!"),
    ///    Err(e) => println!("Error: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn put_limited_on_sale(
        &self,
        item_id: u64,
        uaid: u64,
        price: u64,
    ) -> Result<(), RobloxError> {
        match self
            .put_limited_on_sale_internal(item_id, uaid, price)
            .await
        {
            Ok(x) => Ok(x),
            Err(e) => match e {
                RobloxError::InvalidXcsrf(new_xcsrf) => {
                    self.set_xcsrf(new_xcsrf).await;

                    self.put_limited_on_sale_internal(item_id, uaid, price)
                        .await
                }
                _ => Err(e),
            },
        }
    }

    /// Takes a limited item off sale using the endpoint <https://economy.roblox.com/v1/assets/{item_id}/resellable-copies/{uaid}>.
    ///
    /// # Notes
    /// * Requires a valid roblosecurity.
    /// * Will repeat once if the x-csrf-token is invalid.
    ///
    /// # Return Value Notes
    /// * Will return `Ok(())` if the item was successfully taken off sale.
    ///
    /// # Errors
    /// * All errors under [Standard Errors](#standard-errors).
    /// * All errors under [Auth Required Errors](#auth-required-errors).
    /// * All errors under [X-CSRF-TOKEN Required Errors](#x-csrf-token-required-errors).
    ///
    /// # Example
    /// ```no_run
    /// use roboat::ClientBuilder;
    ///
    /// const ROBLOSECURITY: &str = "roblosecurity";
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ClientBuilder::new().roblosecurity(ROBLOSECURITY.to_string()).build();
    ///
    /// let item_id = 123456789;
    /// let uaid = 987654321;
    ///
    /// match client.take_limited_off_sale(item_id, uaid).await {
    ///    Ok(_) => println!("Successfully took item off sale!"),
    ///    Err(e) => println!("Error: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn take_limited_off_sale(&self, item_id: u64, uaid: u64) -> Result<(), RobloxError> {
        match self.take_limited_off_sale_internal(item_id, uaid).await {
            Ok(x) => Ok(x),
            Err(e) => match e {
                RobloxError::InvalidXcsrf(new_xcsrf) => {
                    self.set_xcsrf(new_xcsrf).await;

                    self.take_limited_off_sale_internal(item_id, uaid).await
                }
                _ => Err(e),
            },
        }
    }

    /// Purchases a limited using  <https://economy.roblox.com/v1/purchases/products/{product_id}>.
    /// Only works on tradeable (legacy) limiteds.
    ///
    /// # Notes
    /// * Requires a valid roblosecurity.
    /// * Will repeat once if the x-csrf-token is invalid.
    ///
    /// # Return Value Notes
    /// * Will return `Ok(())` if the limited was successfully purchased.
    /// * As it will repeat once if the x-csrf-token is invalid, you may want to manually refresh the x-csrf-token
    /// on another thread by using [`Client::force_refresh_xcsrf`].
    ///
    /// # Argument Notes
    /// * `product_id` is the product id of the limited, NOT the item id.
    ///
    /// # Errors
    /// * All errors under [Standard Errors](#standard-errors).
    /// * All errors under [Auth Required Errors](#auth-required-errors).
    /// * All errors under [X-CSRF-TOKEN Required Errors](#x-csrf-token-required-errors).
    /// * [`RobloxError::PurchaseTradableLimitedError`] - Nested inside this error, all variants of [`PurchaseTradableLimitedError`] may be thrown.
    ///
    /// # Example
    /// ```no_run
    /// use roboat::ClientBuilder;
    ///
    /// const ROBLOSECURITY: &str = "roblosecurity";
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ClientBuilder::new().roblosecurity(ROBLOSECURITY.to_string()).build();
    ///
    /// let product_id = 12345679;
    /// let seller_id = 5656565656;
    /// let uaid = 987654321;
    /// let price = 5000;
    ///
    /// let _ = client.purchase_tradable_limited(product_id, seller_id, uaid, price).await?;
    /// println!("Successfully Purchased!");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn purchase_tradable_limited(
        &self,
        product_id: u64,
        seller_id: u64,
        uaid: u64,
        price: u64,
    ) -> Result<(), RobloxError> {
        match self
            .purchase_limited_internal(product_id, price, seller_id, uaid)
            .await
        {
            Ok(x) => Ok(x),
            Err(e) => match e {
                RobloxError::InvalidXcsrf(new_xcsrf) => {
                    self.set_xcsrf(new_xcsrf).await;

                    self.purchase_limited_internal(product_id, price, seller_id, uaid)
                        .await
                }
                _ => Err(e),
            },
        }
    }
}

mod internal {
    use crate::client::{RobloxApi, RobloxError, CONTENT_TYPE, USER_AGENT, XCSRF_HEADER};

    use super::{
        request_types, PurchaseTradableLimitedError, TOGGLE_SALE_API_PART_1, TOGGLE_SALE_API_PART_2,
    };
    use reqwest::header;

    impl RobloxApi {
        pub(super) async fn put_limited_on_sale_internal(
            &self,
            item_id: u64,
            uaid: u64,
            price: u64,
        ) -> Result<(), RobloxError> {
            let formatted_url = format!(
                "{}{}{}{}",
                TOGGLE_SALE_API_PART_1, item_id, TOGGLE_SALE_API_PART_2, uaid
            );

            let cookie = self.cookie_string().await?;

            let json = serde_json::json!({
                "price": price,
            });

            let request_result = self
                .reqwest_client
                .patch(formatted_url)
                .header(header::COOKIE, cookie)
                .header(XCSRF_HEADER, self.xcsrf().await)
                .json(&json)
                .send()
                .await;

            let _ = Self::validate_request_result(request_result).await?;

            // We don't need to do anything, we just need a 200 status code.

            Ok(())
        }

        pub(super) async fn take_limited_off_sale_internal(
            &self,
            item_id: u64,
            uaid: u64,
        ) -> Result<(), RobloxError> {
            let formatted_url = format!(
                "{}{}{}{}",
                TOGGLE_SALE_API_PART_1, item_id, TOGGLE_SALE_API_PART_2, uaid
            );

            let cookie = self.cookie_string().await?;

            let json = serde_json::json!({});

            let request_result = self
                .reqwest_client
                .patch(formatted_url)
                .header(header::COOKIE, cookie)
                .header(XCSRF_HEADER, self.xcsrf().await)
                .json(&json)
                .send()
                .await;

            let _ = Self::validate_request_result(request_result).await?;

            // We don't need to do anything, we just need a 200 status code.

            Ok(())
        }

        pub(super) async fn purchase_limited_internal(
            &self,
            product_id: u64,
            price: u64,
            seller_id: u64,
            uaid: u64,
        ) -> Result<(), RobloxError> {
            let formatted_url = format!(
                "https://economy.roblox.com/v1/purchases/products/{}",
                product_id
            );

            let cookie = self.cookie_string().await?;

            let json = serde_json::json!({
                "expectedCurrency": 1,
                "expectedPrice": price,
                "expectedSellerId": seller_id,
                "userAssetId": uaid,
            });

            let request_result = self
                .reqwest_client
                .post(formatted_url)
                .header(header::COOKIE, cookie)
                .header(XCSRF_HEADER, self.xcsrf().await)
                .header(header::USER_AGENT, USER_AGENT)
                .header(header::CONTENT_TYPE, CONTENT_TYPE)
                .json(&json)
                .send()
                .await;

            let response = Self::validate_request_result(request_result).await?;

            let raw =
                Self::parse_to_raw::<request_types::PurchaseLimitedResponse>(response).await?;

            match raw.purchased {
                true => Ok(()),
                false => match raw.error_msg.as_str() {
                    "You have a pending transaction. Please wait 1 minute and try again." => {
                        Err(RobloxError::PurchaseTradableLimitedError(
                            PurchaseTradableLimitedError::CannotBuyOwnItem,
                        ))
                    }
                    "You already own this item." => Err(RobloxError::PurchaseTradableLimitedError(
                        PurchaseTradableLimitedError::CannotBuyOwnItem,
                    )),
                    "This item is not for sale." => Err(RobloxError::PurchaseTradableLimitedError(
                        PurchaseTradableLimitedError::ItemNotForSale,
                    )),
                    "You do not have enough Robux to purchase this item." => {
                        Err(RobloxError::PurchaseTradableLimitedError(
                            PurchaseTradableLimitedError::NotEnoughRobux,
                        ))
                    }
                    "This item has changed price. Please try again." => {
                        Err(RobloxError::PurchaseTradableLimitedError(
                            PurchaseTradableLimitedError::PriceChanged,
                        ))
                    }
                    _ => Err(RobloxError::PurchaseTradableLimitedError(
                        PurchaseTradableLimitedError::UnknownRobloxErrorMsg(
                            raw.error_msg.as_str().to_string(),
                        ),
                    )),
                },
            }
        }
    }
}
