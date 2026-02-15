use axum::Json;

use rustyfin_models::dto::{CountryInfo, CultureDto, LocalizationOption, ParentalRating};

/// `GET /Localization/Cultures` — Common cultures.
pub async fn get_cultures() -> Json<Vec<CultureDto>> {
    Json(vec![
        CultureDto {
            name: "English (United States)".into(),
            display_name: "English (United States)".into(),
            two_letter_iso_language_name: "en".into(),
            three_letter_iso_language_name: "eng".into(),
        },
        CultureDto {
            name: "French (France)".into(),
            display_name: "Français (France)".into(),
            two_letter_iso_language_name: "fr".into(),
            three_letter_iso_language_name: "fra".into(),
        },
        CultureDto {
            name: "German (Germany)".into(),
            display_name: "Deutsch (Deutschland)".into(),
            two_letter_iso_language_name: "de".into(),
            three_letter_iso_language_name: "deu".into(),
        },
        CultureDto {
            name: "Spanish (Spain)".into(),
            display_name: "Español (España)".into(),
            two_letter_iso_language_name: "es".into(),
            three_letter_iso_language_name: "spa".into(),
        },
        CultureDto {
            name: "Japanese (Japan)".into(),
            display_name: "日本語 (日本)".into(),
            two_letter_iso_language_name: "ja".into(),
            three_letter_iso_language_name: "jpn".into(),
        },
        CultureDto {
            name: "Portuguese (Brazil)".into(),
            display_name: "Português (Brasil)".into(),
            two_letter_iso_language_name: "pt".into(),
            three_letter_iso_language_name: "por".into(),
        },
        CultureDto {
            name: "Chinese (Simplified)".into(),
            display_name: "中文 (简体)".into(),
            two_letter_iso_language_name: "zh".into(),
            three_letter_iso_language_name: "zho".into(),
        },
        CultureDto {
            name: "Korean (Korea)".into(),
            display_name: "한국어 (대한민국)".into(),
            two_letter_iso_language_name: "ko".into(),
            three_letter_iso_language_name: "kor".into(),
        },
        CultureDto {
            name: "Italian (Italy)".into(),
            display_name: "Italiano (Italia)".into(),
            two_letter_iso_language_name: "it".into(),
            three_letter_iso_language_name: "ita".into(),
        },
        CultureDto {
            name: "Russian (Russia)".into(),
            display_name: "Русский (Россия)".into(),
            two_letter_iso_language_name: "ru".into(),
            three_letter_iso_language_name: "rus".into(),
        },
    ])
}

/// `GET /Localization/Countries` — Common countries.
pub async fn get_countries() -> Json<Vec<CountryInfo>> {
    Json(vec![
        CountryInfo {
            name: "United States".into(),
            display_name: "United States".into(),
            two_letter_iso_region_name: "US".into(),
            three_letter_iso_region_name: "USA".into(),
        },
        CountryInfo {
            name: "United Kingdom".into(),
            display_name: "United Kingdom".into(),
            two_letter_iso_region_name: "GB".into(),
            three_letter_iso_region_name: "GBR".into(),
        },
        CountryInfo {
            name: "France".into(),
            display_name: "France".into(),
            two_letter_iso_region_name: "FR".into(),
            three_letter_iso_region_name: "FRA".into(),
        },
        CountryInfo {
            name: "Germany".into(),
            display_name: "Germany".into(),
            two_letter_iso_region_name: "DE".into(),
            three_letter_iso_region_name: "DEU".into(),
        },
        CountryInfo {
            name: "Japan".into(),
            display_name: "Japan".into(),
            two_letter_iso_region_name: "JP".into(),
            three_letter_iso_region_name: "JPN".into(),
        },
        CountryInfo {
            name: "Canada".into(),
            display_name: "Canada".into(),
            two_letter_iso_region_name: "CA".into(),
            three_letter_iso_region_name: "CAN".into(),
        },
        CountryInfo {
            name: "Australia".into(),
            display_name: "Australia".into(),
            two_letter_iso_region_name: "AU".into(),
            three_letter_iso_region_name: "AUS".into(),
        },
    ])
}

/// `GET /Localization/ParentalRatings` — US MPAA + TV ratings.
pub async fn get_parental_ratings() -> Json<Vec<ParentalRating>> {
    Json(vec![
        ParentalRating {
            name: "G".into(),
            value: 1,
        },
        ParentalRating {
            name: "PG".into(),
            value: 5,
        },
        ParentalRating {
            name: "PG-13".into(),
            value: 7,
        },
        ParentalRating {
            name: "R".into(),
            value: 9,
        },
        ParentalRating {
            name: "NC-17".into(),
            value: 10,
        },
        ParentalRating {
            name: "TV-Y".into(),
            value: 1,
        },
        ParentalRating {
            name: "TV-Y7".into(),
            value: 3,
        },
        ParentalRating {
            name: "TV-G".into(),
            value: 4,
        },
        ParentalRating {
            name: "TV-PG".into(),
            value: 5,
        },
        ParentalRating {
            name: "TV-14".into(),
            value: 7,
        },
        ParentalRating {
            name: "TV-MA".into(),
            value: 9,
        },
    ])
}

/// `GET /Localization/Options` — Language options.
pub async fn get_options() -> Json<Vec<LocalizationOption>> {
    Json(vec![
        LocalizationOption {
            name: "English (United States)".into(),
            value: "en-US".into(),
        },
        LocalizationOption {
            name: "Français".into(),
            value: "fr".into(),
        },
        LocalizationOption {
            name: "Deutsch".into(),
            value: "de".into(),
        },
        LocalizationOption {
            name: "Español".into(),
            value: "es".into(),
        },
        LocalizationOption {
            name: "日本語".into(),
            value: "ja".into(),
        },
        LocalizationOption {
            name: "Português (Brasil)".into(),
            value: "pt-BR".into(),
        },
        LocalizationOption {
            name: "中文 (简体)".into(),
            value: "zh-CN".into(),
        },
        LocalizationOption {
            name: "한국어".into(),
            value: "ko".into(),
        },
        LocalizationOption {
            name: "Italiano".into(),
            value: "it".into(),
        },
        LocalizationOption {
            name: "Русский".into(),
            value: "ru".into(),
        },
    ])
}
