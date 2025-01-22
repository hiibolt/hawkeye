// Check if the query parameter `invalid` is set and true,
//  and update the element id `alert-footer` accordingly.
async function build_invalid_alert ( ) {
    let url = new URL(window.location.href);
    let invalid = url.searchParams.get('invalid');

    if ( invalid && invalid == "true" ) {
        let alert_footer = document.getElementById('alert-footer');
        alert_footer.innerHTML = "<i>Invalid Login! Please Try Again.</i>";
    }
}
build_invalid_alert();