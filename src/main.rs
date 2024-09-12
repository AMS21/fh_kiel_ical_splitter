mod error;
mod prelude;

use crate::prelude::*;
use ical::generator::Emitter;
use ical::generator::IcalCalendarBuilder;
use ical::parser::ical::component::IcalEvent;
use once_cell::sync::Lazy;
use regex::Regex;
use regex::RegexBuilder;
use std::collections::BTreeMap;
use std::io::Write;

const CLIENT_USER_AGENT: &str =
    "fh_kiel_ical_splitter/0.1.0 (https://github.com/AMS21/fh_kiel_ical_splitter)";

const CALENDAR_BASE_URL: &str = "https://fh-kalender.de/";

const CACHE_FOLDER: &str = ".cache";

// 1 request every 5 second
const DOWNLOAD_DELAY: std::time::Duration = std::time::Duration::from_secs(5);

// How long to wait before retrying a download
const DOWNLOAD_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(30);

// How often to retry a download before failing
const MAX_RETRIES: usize = 10;

#[derive(Debug)]
struct CalendarEntry {
    pub events: Vec<IcalEvent>,
    pub department: String,
    pub year: String,
    pub institute: String,
}

fn get_website(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
    let cache_folder = std::path::Path::new(CACHE_FOLDER);
    debug_assert!(cache_folder.exists(), "Cache folder does not exist!");

    let cache_file = std::path::Path::new(CACHE_FOLDER).join(url.replace('/', "_"));

    // Check if the cache file exists and load content from disk if it does
    if cache_file.exists() {
        return Ok(std::fs::read_to_string(cache_file)?);
    }

    // If the cache file doesn't exist, actually send a request and cache it
    let mut response = client.get(url).send()?;

    for try_count in 0..MAX_RETRIES {
        // Check if the request was successful
        if response.status().is_success() {
            break;
        }

        warn!(
            "[{}/{}] Request for '{}' failed with status: {}, waiting {} seconds before retrying",
            try_count + 1,
            MAX_RETRIES,
            url,
            response.status(),
            DOWNLOAD_RETRY_DELAY.as_secs()
        );
        if try_count == MAX_RETRIES - 1 {
            return Err(error::Error::RequestFailed(response.status()));
        }

        // Wait before retrying the download
        std::thread::sleep(DOWNLOAD_RETRY_DELAY);

        // Send next request
        response = client.get(url).send()?;
    }

    // Check if the response body is empty
    let response_body = response.text()?;
    if response_body.is_empty() {
        error!("Response body is empty");
        return Err(error::Error::EmptyResponse);
    }

    // Cache the response
    std::fs::write(cache_file, &response_body)?;

    // Wait a bit to not spam the server when downloading
    std::thread::sleep(DOWNLOAD_DELAY);

    Ok(response_body)
}

#[allow(clippy::unwrap_used)]
fn extract_components_from_url(url: &str) -> Result<(String, String, String)> {
    // Sample link: /files/iue/WiSe_2425/semester_1/1_Sem_Elektrotechnik_Gruppe_1.ics
    static URL_COMPONENTS_EXTRACT_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"/files/(.*?)/(.*?)/(.*?)/.*?\.ics").unwrap());

    let captures = URL_COMPONENTS_EXTRACT_REGEX
        .captures(url)
        .ok_or(error::Error::InvalidUrl(url.to_owned()))?;

    let department = captures.get(1).unwrap().as_str();
    let year = captures.get(2).unwrap().as_str();
    let institute = captures.get(3).unwrap().as_str();

    Ok((year.to_owned(), department.to_owned(), institute.to_owned()))
}

fn extract_department_links_from_website(website_source: &str) -> Vec<String> {
    // Sample: <a href="/informatik-elektrotechnik" role="button" class="contrast" style="display: grid; place-items: center; margin-bottom: 1rem;"> Informatik und Elektrotechnik </a>
    #[allow(clippy::unwrap_used)]
    static DEPARTMENT_LINK_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new("<a href=\"/([a-zA-Z-]+?)\" role=\"button\"").unwrap());

    let mut links = vec![];

    DEPARTMENT_LINK_REGEX
        .captures_iter(website_source)
        .map(|c| c.extract())
        .for_each(|(_, [link])| {
            links.push(link.to_owned());
        });

    links
}

#[allow(clippy::unwrap_used)]
fn is_event_already_present(new_event: &IcalEvent, events: &Vec<IcalEvent>) -> bool {
    let new_event_start = new_event
        .properties
        .iter()
        .find(|p| p.name == PROPERTY_NAME_DTSTART)
        .map(|p| p.value.clone())
        .unwrap()
        .unwrap();
    let new_event_end = new_event
        .properties
        .iter()
        .find(|p| p.name == PROPERTY_NAME_DTEND)
        .map(|p| p.value.clone())
        .unwrap()
        .unwrap();

    for event in events {
        let event_start = event
            .properties
            .iter()
            .find(|p| p.name == PROPERTY_NAME_DTSTART)
            .map(|p| p.value.clone())
            .unwrap()
            .unwrap();
        let event_end = new_event
            .properties
            .iter()
            .find(|p| p.name == PROPERTY_NAME_DTEND)
            .map(|p| p.value.clone())
            .unwrap()
            .unwrap();

        if new_event_start == event_start && new_event_end == event_end {
            return true;
        }
    }

    false
}

const PROPERTY_NAME_SUMMARY: &str = "SUMMARY";
const PROPERTY_NAME_DTSTART: &str = "DTSTART";
const PROPERTY_NAME_DTEND: &str = "DTEND";

#[allow(clippy::too_many_lines)]
fn main() -> Result<()> {
    // Install color_eyre error handler
    color_eyre::install()?;

    // Initialize tracing
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber)?;

    // Ensure cache directory exists
    std::fs::create_dir_all(CACHE_FOLDER)?;

    // Build our blocking client
    let client = reqwest::blocking::Client::builder()
        .user_agent(CLIENT_USER_AGENT)
        .https_only(true)
        .build()?;

    // Download main site
    let main = get_website(&client, CALENDAR_BASE_URL)?;

    // Extract all institute links
    let institute_links = extract_department_links_from_website(&main);

    info!("Successfully found {} departments", institute_links.len());

    // Build regex
    let ics_link_regex = RegexBuilder::new("a href=\"(.*?\\.ics)\"")
        .case_insensitive(true)
        .build()?;

    let mut number_of_found_calendars: u32 = 0;
    let mut total_number_of_events: u32 = 0;
    let mut map: BTreeMap<String, CalendarEntry> = BTreeMap::new();

    for link in &institute_links {
        // Download the institute sub page
        let institute_url = CALENDAR_BASE_URL.to_owned() + link;
        let institute_page = get_website(&client, &institute_url)?;

        // Iterate through all ics links on the insititutes page
        for (_, [link]) in ics_link_regex
            .captures_iter(institute_page.as_str())
            .map(|c| c.extract())
        {
            // Ignore any links that only point to teachers
            if link.contains("/dozenten/") {
                continue;
            }

            // Extract components from URL
            let (year, department, institute) = extract_components_from_url(link)?;

            // Download the calendar file
            let url = CALENDAR_BASE_URL.to_owned() + link;
            let ics_file = get_website(&client, &url)?;

            let ical_reader = ical::IcalParser::new(ics_file.as_bytes());

            // Print all events
            for calendar in ical_reader {
                match calendar {
                    Ok(calendar) => {
                        number_of_found_calendars += 1;

                        // Iterate through all events of that calendar
                        for event in calendar.events {
                            // Find summary
                            let summary = event
                                .properties
                                .iter()
                                .find(|p| p.name == PROPERTY_NAME_SUMMARY)
                                .map(|p| p.value.clone());

                            if let Some(Some(name)) = summary {
                                // Ignore festive days
                                if name.contains("Feiertag") {
                                    continue;
                                }

                                // Append to map
                                if let std::collections::btree_map::Entry::Vacant(e) =
                                    map.entry(name.clone())
                                {
                                    // Create new map entry for this course
                                    e.insert(CalendarEntry {
                                        events: vec![event],
                                        department: department.clone(),
                                        year: year.clone(),
                                        institute: institute.clone(),
                                    });
                                } else if let Some(calendar_entry) = map.get_mut(&name) {
                                    // Don't add any duplicate events
                                    if !is_event_already_present(&event, &calendar_entry.events) {
                                        calendar_entry.events.push(event);
                                    }
                                }

                                total_number_of_events += 1;
                            } else {
                                error!("Calendar event is missing summary");
                            }
                        }
                    }
                    Err(err) => {
                        error!("Parse error for event: {err}");
                    }
                }
            }
        }
    }

    info!(
        "Successfully loaded {} courses from {} calendars",
        map.len(),
        number_of_found_calendars
    );

    let number_of_courses = &map.len();

    let mut index_file = std::fs::File::create("index.html")?;

    writeln!(
        &mut index_file,
        "<!DOCTYPE html><html lang=\"de\">
<head>
<meta charset=\"UTF-8\">
<meta name=\"description\" content=\"iCalender der Vorlesungspläne der FH-Kiel\">
<title>iCalender der Vorlesungspläne der FH-Kiel für jedes Module</title>

<style>
</style>

</head>

<body>
<h1>Vorlesungspläne der FH-Kiel für jedes Module</h1>
<h4>Absolut kein Gewähr auf <b>Richtigkeit</b> oder <b>Vollständigkeit</b>. Überprüft vor der Nutzung bitte die offiziellen FH-Kiel-Pläne.</h4>
<div>
<ul>"
    )?;

    // Generate output
    for (module, entries) in map {
        let mut calendar = IcalCalendarBuilder::version("2.0")
            .gregorian()
            .prodid(format!(
                "-//Veranstaltungsplan der FH Kiel//{}//{}//{}//{}//",
                entries.year, entries.department, entries.institute, module
            ))
            .build();

        // Add the specific events
        for entry in entries.events {
            calendar.events.push(entry);
        }

        // Create folder
        let directory_path = format!(
            "files/{}/{}/{}",
            entries.year, entries.department, entries.institute
        );
        std::fs::create_dir_all(&directory_path)?;

        // Write to file
        let file_name = format!("{}/{}.ics", directory_path, module.replace('/', "_"));
        std::fs::write(&file_name, calendar.generate())?;

        // Create link in html file
        #[cfg(feature = "github_pages")]
        writeln!(
            &mut index_file,
            "<li> <a href=\"/fh_kiel_ical_splitter/{file_name}\">{module}</a> </li>"
        )?;

        #[cfg(not(feature = "github_pages"))]
        writeln!(
            &mut index_file,
            "<li> <a href=\"/{file_name}\">{module}</a> </li>"
        )?;

        info!(
            "Successfully created calendar for module '{module}' with {} events",
            calendar.events.len()
        );
    }

    writeln!(
        &mut index_file,
        "</ul>
</div>
<footer>
<p>Quelle: <a href=\"https://fh-kalender.de/\">https://fh-kalender.de/</a></p>
<p>Generiert am: {}</p>
</footer>
</body>
</html>",
        chrono::Local::now().format("%d.%m.%Y %H:%M:%S")
    )?;

    info!(
        "Successfully generated {} calendars for {} departments with a total of {} events",
        number_of_courses,
        institute_links.len(),
        total_number_of_events
    );

    Ok(())
}
